# L8C Test Plan: Storage Mode Capability Validation

Status: implemented

Parent plan:
`docs/architecture/implementation-plans/M4/L8/l8c-storage-mode-capability-validation-implementation-plan.md`

## Goal

Prove that L8C validates requested storage modes against backend capability
facts before any lifecycle open/create side effects.

The tests must fail if L8C:

1. duplicates or drifts from the backend capability matrix;
2. requires durable-only capabilities for cache mode;
3. accepts durable local modes without append, durable publish, durable sync, or
   writer-lock support;
4. loses the difference between durable standard and durable always policy;
5. accepts object-durable candidate without consistent metadata and a fencing
   primitive;
6. claims object-durable candidate is production durable local behavior;
7. reports capability mismatch only as unstructured display text;
8. omits either the complete required capability list or exact missing list from
   a capability mismatch;
9. calls backend read/write/list/publish/append/lock methods during capability
   validation;
9. creates durable objects, temporary objects, service instances, or runtime
   state before capability validation succeeds;
10. imports product, engine, raw IO, follower, StrataHub, or service-assembly
    vocabulary into the L8C production path.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/capability.rs` for direct
   capability validation tests.
2. `crates/storage-next/src/lifecycle/tests/mod.rs` only for shared helpers and
   existing lifecycle tests.
3. `crates/storage-next/src/testkit/lifecycle/` for generated capability
   scripts and lifecycle scaffold counters.
4. `crates/storage-next/tests/lifecycle_properties.rs` for generated property
   assertions.
5. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
6. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for the
   L8C verification and sensitivity-probe entry after implementation.

Do not add tests that require durable local service assembly, manifest
creation, WAL open, snapshot publication, L6 mutation, L7 commit execution,
maintenance execution, retention, quarantine, repair, or close side effects.

Do not add tests whose only assertion is that plan documents exist or link to
each other. L8C tests should assert executable storage behavior.

## Direct Unit Tests

### 1. Mode Mapping

Required cases:

1. lifecycle `StorageMode::Cache` maps to `StorageModeRequest::cache()`;
2. lifecycle `StorageMode::DurableLocalStandard` maps to
   `StorageModeRequest::durable_local(DurabilityPolicy::Standard)`;
3. lifecycle `StorageMode::DurableLocalAlways` maps to
   `StorageModeRequest::durable_local(DurabilityPolicy::Always)`;
4. lifecycle `StorageMode::ObjectDurableCandidate` maps to
   `StorageModeRequest::object_durable_candidate()`;
5. every lifecycle storage mode is covered by an exhaustive match;
6. mapping preserves the original lifecycle mode in the accepted/rejected facts.

Assertions:

1. no mode silently maps to cache as a default;
2. durable always does not collapse to durable standard in the returned facts;
3. object durable candidate does not collapse to durable local.

### 2. Cache Capability Acceptance

Required cases:

1. exact `CACHE_MODE_REQUIREMENTS` accepts cache mode;
2. `BASIC_OBJECT_BACKEND_CAPABILITIES` accepts cache mode;
3. memory backend capabilities accept cache mode;
4. extra durable capabilities do not make cache mode reject;
5. cache accepted outcome has no durability policy;
6. cache accepted outcome has no object-candidate fence mode.

Cache mode must not require:

1. `ObjectMetadata`;
2. `AppendObject`;
3. `DurablePublish`;
4. `DurableSync`;
5. `SingleWriterLock`;
6. `ConditionalPublish`;
7. `ConsistentList`;
8. `MonotonicMetadata`.

### 3. Cache Capability Rejection

For each required cache capability, construct capabilities missing only that
one capability and assert rejection:

1. `ReadObject`;
2. `ReadRange`;
3. `WriteObject`;
4. `DeleteObject`;
5. `ListPrefix`.

Assertions:

1. rejected outcome/error identifies the requested cache mode;
2. missing set contains the absent capability;
3. missing set does not include durable-only capabilities;
4. display text remains storage-shaped and bounded.

### 4. Durable Local Standard Acceptance

Required cases:

1. exact `DURABLE_LOCAL_MODE_REQUIREMENTS` accepts durable standard;
2. localfs capabilities accept durable standard on supported platforms;
3. extra conditional/object capabilities do not change the selected durable
   policy;
4. accepted outcome records `DurabilityPolicy::Standard`;
5. accepted outcome has no object-candidate fence mode.

### 5. Durable Local Always Acceptance

Required cases:

1. exact `DURABLE_LOCAL_MODE_REQUIREMENTS` accepts durable always;
2. localfs capabilities accept durable always on supported platforms;
3. accepted outcome records `DurabilityPolicy::Always`;
4. accepted outcome differs from durable standard only by policy at L8C;
5. no per-commit durable barrier is attempted during validation.

### 6. Durable Local Rejection

For both durable standard and durable always, construct missing-only-one cases
for:

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

Assertions:

1. rejected outcome/error identifies the durable mode and policy;
2. missing set contains exactly the absent capability when all others are
   present;
3. durable publish and durable sync are both required;
4. writer-lock absence rejects before any writer-lock acquisition attempt;
5. memory backend capabilities reject durable local modes with the expected
   durable missing set.

### 7. Object Durable Candidate Acceptance

Required accepted cases:

1. base object-candidate requirements plus `ConditionalPublish`;
2. base object-candidate requirements plus `ConditionalCreate` and
   `ConditionalUpdate`;
3. all three fencing capabilities present;
4. accepted outcome records `StorageMode::ObjectDurableCandidate`;
5. accepted outcome records the selected fence mode deterministically;
6. accepted outcome does not report `DurabilityPolicy::Standard` or
   `DurabilityPolicy::Always`.

If all three fencing capabilities are present, tests must pin the documented
preference order.

### 8. Object Durable Candidate Rejection

Required rejected cases:

1. missing `ConsistentList`;
2. missing `MonotonicMetadata`;
3. missing `ObjectMetadata`;
4. missing both fence alternatives;
5. only `ConditionalCreate` present;
6. only `ConditionalUpdate` present;
7. memory backend/basic object capabilities;
8. durable-local capabilities without object-consistency capabilities.

Assertions:

1. missing set identifies base missing capabilities;
2. missing fence reports `ConditionalPublish` when no fence alternative exists;
3. partial create/update pair reports the missing counterpart;
4. object candidate remains explicitly candidate-tagged in failures;
5. display text does not claim production object durability.

### 9. Side-Effect-Free Preflight

Use a counting fake backend that exposes configurable capabilities and records
calls to every backend method.

Required cases:

1. accepted cache validation calls only `capabilities()`;
2. rejected cache validation calls only `capabilities()`;
3. accepted durable standard validation calls only `capabilities()`;
4. rejected durable standard validation calls only `capabilities()`;
5. accepted durable always validation calls only `capabilities()`;
6. rejected durable always validation calls only `capabilities()`;
7. accepted object candidate validation calls only `capabilities()`;
8. rejected object candidate validation calls only `capabilities()`.

Forbidden calls during L8C validation:

1. `read_object`;
2. `read_range`;
3. `write_object`;
4. `delete_object`;
5. `list_prefix`;
6. `object_metadata`;
7. `append_object`;
8. `sync_object`;
9. `publish_object`;
10. `conditional_create`;
11. `conditional_update`;
12. `acquire_writer_lock`;
13. any future backend mutation or durable-service hook.

Assertions:

1. capability validation has no durable object side effects;
2. rejection does not leave temporary objects;
3. rejection does not acquire or release writer guards;
4. accepted validation still does not assemble services.

### 10. Error And Outcome Shape

Required cases:

1. accepted outcome exposes storage mode;
2. accepted outcome exposes requested durability policy where relevant;
3. accepted outcome exposes backend capabilities used;
4. rejected outcome/error exposes missing capabilities as typed facts;
5. rejected outcome/error preserves lower-layer source only if a backend
   capability read can fail in the final implementation;
6. display text is bounded and product-neutral;
7. debug text is bounded and product-neutral;
8. equality tests do not depend on dynamic display strings.

Forbidden display/debug vocabulary:

1. `Database::open`;
2. `OpenOptions`;
3. `public maintenance`;
4. `Follower`;
5. `StrataHub`;
6. `VersionedValue`;
7. `EntityRef`;
8. `JsonValue`;
9. `Graph`;
10. `Vector`;
11. `Search`;
12. `Embedding`;
13. `Inference`;
14. `TransactionContext`.

### 11. Regression Against Existing Lower-Layer Matrix

L8C direct tests should assert that lifecycle validation agrees with existing
lower-layer `StorageModeRequest` behavior:

1. for cache accepted capabilities;
2. for durable standard accepted capabilities;
3. for durable always accepted capabilities;
4. for object candidate conditional-publish acceptance;
5. for object candidate create/update pair acceptance;
6. for object candidate missing-fence rejection.

This is not a doc-link test. It is an executable parity check against the
existing capability request implementation.

## Generated Property Harness

Extend the L8A/L8B lifecycle property route.

### Required Counters

Add counters for:

1. accepted capability validations;
2. rejected capability validations;
3. cache capability cases;
4. durable-standard capability cases;
5. durable-always capability cases;
6. object-candidate capability cases;
7. missing-capability cases;
8. object-candidate conditional-publish fence cases;
9. object-candidate create/update fence cases;
10. side-effect-free preflight cases;
11. input-derived capability cases.

### Script Shape

The existing L8A/L8B decoder uses bytes `0..12`. Extend it without changing
those meanings:

```text
byte 13: capability validation storage-mode selector
bytes 14-15: capability bitmask low/high selector
byte 16: object-candidate fence selector
byte 17: remove-one-required-capability selector
byte 18: preflight backend behavior selector
```

The contract may run deterministic canonical capability cases first, but it
must also run at least one input-derived capability case and count it
separately.

### Generated Assertions

For every generated script:

1. validation returns typed results or typed errors, never panics;
2. every storage mode appears in either direct canonical cases or input-derived
   cases;
3. at least one accepted and one rejected capability route are counted;
4. missing-capability routes have nonempty missing sets;
5. side-effect-free preflight cases call only `capabilities()`;
6. accepted object-candidate cases record a fence mode;
7. rejected object-candidate cases do not record production durable policy.

## Source Boundary Regression

Run and extend `lifecycle_source_guard` if needed.

L8C production lifecycle code may import:

1. backend capability facts;
2. backend capability requirement constants;
3. storage mode request facts from `config::mode`;
4. lifecycle-local facts, errors, and results.

L8C production lifecycle code must not import:

1. `crate::service`;
2. `crate::layout::ObjectLayout`;
3. `crate::format`;
4. `crate::table`;
5. `crate::branch`;
6. `crate::commit`;
7. engine crates;
8. raw filesystem/path/environment APIs;
9. product DTO vocabulary;
10. follower or StrataHub vocabulary.

Because later L8 slices will intentionally import services and commit/branch
types, any L8C-specific service-absence guard should either be local to the
L8C test module or be written with an explicit future-removal note.

## Non-Behavior Assertions

The L8C suite should prove by fake-backend counters and source boundaries:

1. no manifest create/load/publish;
2. no WAL append/open/replay/truncate;
3. no snapshot/checkpoint write/load;
4. no table object publish;
5. no quarantine inventory publish/read;
6. no writer-lock acquisition;
7. no L6 branch mutation;
8. no L7 commit/replay execution;
9. no maintenance queue execution;
10. no close side effects.

## Sensitivity Probes

Record in the L8C porting-log entry after implementation:

| Probe | Mutation | Expected failing test |
|---|---|---|
| Cache durable creep | Add `DurablePublish` to cache requirements | Cache acceptance/rejection matrix |
| Cache metadata creep | Add `ObjectMetadata` to cache requirements | Cache accepts browser-like capabilities |
| Durable append omitted | Remove `AppendObject` from durable requirements | Durable missing-append rejection |
| Durable sync omitted | Remove `DurableSync` from durable requirements | Durable missing-sync rejection |
| Writer guard omitted | Remove `SingleWriterLock` from durable requirements | Durable missing-writer-lock rejection |
| Always collapsed | Map durable always to standard without preserving policy | Durable always policy preservation |
| Object consistency omitted | Remove `ConsistentList` or `MonotonicMetadata` from object candidate | Object-candidate missing consistency rejection |
| Object fence omitted | Accept object candidate with no fence primitive | Object-candidate missing-fence rejection |
| Side-effect before validation | Call metadata/list/publish/lock during validation | Side-effect-free preflight test |
| Product wording leak | Add product open/follower/StrataHub term | lifecycle source guard |

## Verification Commands

Mandatory L8C commands:

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

If localfs-specific capability coverage is added:

```text
cargo test -p strata-storage-next --all-features --locked lifecycle
```

Optional before L8C closeout if `cargo-hack` is installed:

```text
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

## Deferred To Later Slices

1. Cache-mode runtime open/close: L8D.
2. Durable local service assembly and writer-lock acquisition: L8E.
3. Manifest load/create/publish: L8E.
4. WAL, snapshot, table, timeline, and quarantine recovery: L8F.
5. L7 replay/bootstrap and recovery-health finalization: L8G.
6. Maintenance executor and task execution: L8H-L8K.
7. Retention, quarantine, purge, repair, close, crash, fuzz, and closeout:
   L8L-L8P.

## Close Criteria

L8C test coverage is complete when:

1. direct tests cover every storage mode;
2. direct tests cover every required cache capability;
3. direct tests cover every required durable local capability;
4. direct tests cover both object-candidate fence alternatives;
5. direct tests cover partial object-candidate fence failures;
6. direct tests prove missing capabilities are typed facts;
7. fake-backend tests prove validation calls only `capabilities()`;
8. generated properties count accepted and rejected capability cases;
9. generated properties include input-derived capability routes;
10. source guards still prevent product/engine/raw-IO/service-assembly drift;
11. mandatory verification commands pass;
12. the porting log records the implemented capability matrix and sensitivity
    probes.
