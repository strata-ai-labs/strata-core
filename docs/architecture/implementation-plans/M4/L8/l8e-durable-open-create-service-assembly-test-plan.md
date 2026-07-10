# L8E Test Plan: Durable Open/Create Service Assembly

Status: implemented

Parent plan:
`docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-implementation-plan.md`

## Goal

Prove that L8E assembles durable-local storage services safely and stops before
recovery. Tests should focus on executable storage behavior: mode validation,
capability ordering, writer-lock lifetime, manifest open/create, WAL active
segment selection, service-bundle facts, state/admission, and lower-layer error
classification.

The tests must fail if L8E:

1. accepts cache or object-durable candidate through durable assembly;
2. performs durable side effects before L8C capability preflight;
3. loads or creates the manifest before holding the writer guard;
4. uses any writer-lock name other than `ObjectLayout::writer_lock()`;
5. drops the writer guard before returning the durable shell;
6. ignores manifest database-id or codec-id mismatch;
7. opens WAL on a hardcoded segment instead of manifest `active_wal_segment`;
8. collapses durable standard and durable always into one policy;
9. replays WAL, reads snapshots, reads table objects, or reconciles quarantine
   during assembly;
10. reports the durable runtime as `Open` before recovery/bootstrap;
11. allows ordinary reads or commits before L8F/L8G finish recovery;
12. imports product, engine, follower, IPC, primitive, raw filesystem, or
   StrataHub vocabulary.

Do not add tests whose only assertion is that plan documents exist or link to
each other. L8E tests should assert implementation behavior.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/durable.rs` for direct durable
   assembly tests.
2. `crates/storage-next/src/lifecycle/tests/mod.rs` only for shared helpers.
3. `crates/storage-next/src/testkit/lifecycle/durable.rs` for generated durable
   assembly scripts and counters.
4. `crates/storage-next/tests/lifecycle_properties.rs` for generated property
   assertions.
5. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
6. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for the
   L8E verification and sensitivity-probe entry after implementation.

Do not add integration tests that require L8F recovery, L8G replay/bootstrap,
maintenance execution, checkpointing, retention, quarantine mutation, repair,
close drain, public L9 APIs, or product callbacks.

## Direct Unit Tests

### 1. Durable Request Validation

Required cases:

1. durable local standard plan accepts;
2. durable local always plan accepts;
3. cache plan rejects;
4. object durable candidate plan rejects;
5. invalid codec id rejects through `StorageOpenPlan`;
6. zero branch generation rejects through the L7 generation type;
7. invalid branch runtime config rejects before service mutation;
8. invalid commit runtime config rejects before service mutation;
9. invalid WAL service config rejects before WAL open;
10. plan codec id and WAL service codec id mismatch rejects before manifest
    create;
11. a non-identity durable codec is rejected before manifest create until the
    WAL service supports non-identity codecs.

Assertions:

1. rejection returns no durable shell;
2. errors are typed lifecycle errors or lower-layer typed errors;
3. display text is storage-shaped and product-neutral.

### 2. Capability Preflight Ordering

Use a counting fake backend that records every backend call.

Required cases:

1. accepted durable standard calls `capabilities()` before any other backend
   method;
2. accepted durable always calls `capabilities()` before any other backend
   method;
3. missing durable capability rejects after `capabilities()` and before lock;
4. cache/object candidate rejection through durable request validation does not
   acquire the writer lock;
5. capability rejection does not construct manifest, WAL, snapshot, table,
   checkpoint, or quarantine services.

Forbidden calls before successful capability preflight:

1. `acquire_writer_lock`;
2. `read_object`;
3. `write_object`;
4. `publish_object`;
5. `append_object`;
6. `sync_object`;
7. `list_prefix`;
8. `object_metadata`;
9. `delete_object`.

Assertions:

1. no temporary object is left behind after capability rejection;
2. no writer guard is acquired or released after capability rejection;
3. rejected missing-capability errors list typed missing capabilities.

### 3. Writer Guard Ordering And Lifetime

Required cases:

1. writer guard uses `ObjectLayout::writer_lock()`;
2. writer guard is acquired after capability preflight;
3. writer guard is acquired before manifest load;
4. writer guard is acquired before manifest create;
5. writer guard is acquired before WAL open;
6. writer-lock backend failure rejects before manifest mutation;
7. writer-lock layout failure maps to `LifecycleLowerLayer::Layout`;
8. backend lock failure maps to `LifecycleLowerLayer::Backend`;
9. returned shell keeps the guard held;
10. dropping the shell releases the guard;
11. a second opener cannot acquire the localfs writer lock while the first shell
    is alive on supported platforms;
12. the second opener can acquire after the first shell is dropped.

Assertions:

1. no lifecycle code contains the literal `"locks/writer"` outside tests;
2. writer guard is not exposed through public or crate-root APIs;
3. writer guard release happens by RAII unless a later close path explicitly
   consumes it.

### 4. New Database Manifest Create

Required cases:

1. absent manifest creates initial manifest;
2. created manifest has requested database id;
3. created manifest has requested codec id;
4. created manifest active WAL segment is `1`;
5. created manifest has no snapshot watermark;
6. created manifest has no snapshot id;
7. created manifest has no flush watermark;
8. open disposition is `Created`;
9. manifest create uses durable publish create;
10. manifest create validates `PublishOutcome` object, size, and durability via
    lower service guarantees.

Assertions:

1. lifecycle does not encode manifest bytes directly;
2. lifecycle does not publish replacement for first create;
3. manifest facts are carried into assembly facts exactly.

### 5. Existing Manifest Load

Required cases:

1. existing manifest loads without replacement;
2. requested database id must match existing manifest;
3. requested codec id must match existing manifest;
4. active WAL segment is copied from manifest;
5. snapshot watermark is preserved;
6. snapshot id is preserved;
7. flush watermark is preserved;
8. open disposition is `OpenedExisting`;
9. corrupt manifest bytes fail closed;
10. future manifest version fails closed;
11. pre-V1 manifest version fails closed;
12. manifest read `NotFound` remains the create path, not corruption.

Assertions:

1. no manifest replacement is published during existing open;
2. identity mismatch happens before WAL open;
3. lower `ManifestServiceError` remains in the source chain.

### 6. Manifest Create Races And Publish Faults

Required cases:

1. load absent then create returns precondition failed and reload exact matching
   manifest succeeds as `OpenedExisting`;
2. load absent then create returns precondition failed and reload mismatch fails
   closed;
3. load absent then create returns `FailedBeforeVisibility`;
4. load absent then create returns `VisibilityUnknown`;
5. load absent then create returns `VisibleDurabilityUnconfirmed`;
6. load absent then create returns unsupported durable publish;
7. publish error source chain includes backend source;
8. uncertain create is classified distinctly from failed-before-visibility.

Assertions:

1. retry logic never blindly overwrites an existing manifest;
2. uncertainty returns typed facts for later recovery/close handling;
3. writer guard remains held until the shell/error path is dropped.

### 7. WAL Service Open

Required cases:

1. new database opens WAL segment `1`;
2. existing manifest opens the manifest active segment;
3. active segment `0` in a manifest fails through manifest decode/validation;
4. WAL open receives requested database id;
5. WAL open receives requested codec id through `WalServiceConfig`;
6. WAL open receives `DurabilityPolicy::Standard` for durable standard;
7. WAL open receives `DurabilityPolicy::Always` for durable always;
8. WAL header database mismatch fails closed;
9. WAL segment id overflow or invalid active segment maps to service lower
   layer;
10. WAL open failure after manifest create preserves partial assembly facts.

Assertions:

1. no WAL record replay occurs;
2. no L7 replay hook is called;
3. no visible version advances;
4. no commit allocator catches up.

### 8. Service Bundle Assembly

Required cases:

1. database manifest service is present;
2. table manifest service is present;
3. WAL service is present;
4. WAL sidecar service is present;
5. snapshot service is present;
6. table object write service is present;
7. table object reader service is present;
8. checkpoint service is present;
9. quarantine service is present;
10. all services use the same backend;
11. services are constructed exactly once;
12. services are not exposed outside crate-private lifecycle surfaces.

Assertions:

1. no service reads snapshots during assembly;
2. no service reads table objects during assembly;
3. no service lists quarantine inventory during assembly;
4. no checkpoint operation runs during assembly.

### 9. L6/L7 Recovery Shell Baseline

Required cases:

1. branch recovery target starts empty;
2. branch registry contains only the configured initial branch if the shell
   registers it at this stage;
3. visible tracker starts at `CommitVersion::ZERO`;
4. version allocator starts at `CommitVersion::ZERO`;
5. timestamp guard starts empty;
6. unresolved durable gate starts empty;
7. L7 durable commit runtime is not executed;
8. L7 replay runtime is not executed.

Assertions:

1. L8E does not install manifest facts as committed rows;
2. L8E does not synthesize timeline rows;
3. L8E does not bootstrap allocator or visible facts from manifest watermarks.

### 10. Lifecycle State And Admission

Required cases:

1. durable assembly starts at `New`;
2. open request transitions to `Opening`;
3. successful service assembly transitions to `Recovering`;
4. returned shell state is `Recovering`;
5. ordinary read is rejected while recovering;
6. commit is rejected while recovering;
7. ordinary maintenance is rejected while recovering;
8. recovery step is admitted while recovering;
9. health query remains admitted;
10. failed assembly records a typed lifecycle failure fact where applicable.

Assertions:

1. L8E does not create a final `StorageOpenOutcome` claiming recovered
   visibility;
2. L8E does not report maintenance readiness;
3. final open outcome is deferred to L8G.

### 11. Durable Standard Vs Durable Always

Required cases:

1. durable standard accepted outcome records `DurabilityPolicy::Standard`;
2. durable always accepted outcome records `DurabilityPolicy::Always`;
3. service bundle shape is otherwise identical;
4. WAL service policy differs according to mode;
5. generated tests exercise both modes from input-derived bytes.

Assertions:

1. durable always never silently downgrades to standard;
2. standard never claims per-commit force durability at assembly;
3. capability requirements remain shared with L8C.

### 12. No Recovery Or Maintenance Side Effects

Use a backend spy and optional service spies.

Forbidden during L8E assembly:

1. WAL record iteration/replay;
2. WAL partial-tail repair;
3. WAL retention/truncation;
4. snapshot load unless a later implementation explicitly places it in L8F;
5. table object read;
6. branch/table manifest recovery;
7. quarantine inventory load/reconcile;
8. checkpoint creation;
9. flush;
10. compaction;
11. materialization;
12. retention/reclaim/purge/repair;
13. background task spawning;
14. product primitive callback.

Assertions:

1. side-effect counters remain zero for each forbidden category;
2. source guard catches direct imports of product recovery hooks;
3. generated contract has a counter for no-recovery assembly.

### 13. Error Source Chains

Required lower-layer failures:

1. backend capability mismatch;
2. writer-lock backend failure;
3. manifest read backend failure;
4. manifest decode failure;
5. manifest publish failure;
6. WAL config validation failure;
7. WAL open backend failure;
8. branch shell construction failure;
9. commit shell construction failure.

Assertions:

1. `LifecycleError::source()` returns the lower-layer error where available;
2. nested source chain reaches the original backend error for backend-backed
   failures;
3. equality tests compare typed category/reason rather than dynamic display;
4. display text is bounded and product-neutral.

## Generated And Property Tests

Extend the lifecycle generated harness with durable assembly scripts.

Generated dimensions:

1. storage mode: cache, durable standard, durable always, object candidate;
2. capability set: exact, extra, missing each durable requirement;
3. manifest state: absent, valid existing, corrupt, codec mismatch, database id
   mismatch, precondition race;
4. manifest recovery facts: no checkpoint, snapshot pair, flush watermark,
   non-default active WAL segment;
5. durability policy: standard and always;
6. writer-lock outcome: success, unavailable, unsupported;
7. WAL outcome: success, database mismatch, segment corruption, backend
   failure;
8. post-assembly action: recovery step, ordinary read, commit, health query.

Required counters:

1. durable standard accepted;
2. durable always accepted;
3. non-durable rejected;
4. missing capability rejected before lock;
5. writer lock acquired;
6. writer lock failure;
7. manifest created;
8. manifest opened existing;
9. manifest identity mismatch;
10. manifest publish uncertainty;
11. WAL opened active segment;
12. WAL open failure;
13. service bundle assembled;
14. shell remains recovering;
15. ordinary read/commit rejected while recovering;
16. no recovery side effects.

The property harness must not satisfy all counters through a fixed canonical
script before consuming generated bytes. If a canonical smoke script is needed,
keep it in a separate direct test and keep generated counters tied to
input-derived operations.

## Source Guard Tests

Extend `lifecycle_source_guard` to check:

1. `lifecycle/durable.rs` has no raw `std::fs`, `std::path::Path`, or
   `std::env` imports;
2. no product or engine module imports production lifecycle code;
3. no follower, IPC, primitive registry, search, graph, vector, inference, or
   StrataHub vocabulary appears in durable lifecycle production code;
4. writer-lock literals are not hardcoded in lifecycle production code;
5. durable assembly imports `service` only from `lifecycle/durable.rs`, not
   from cache or capability slices;
6. L6/L7 lower layers do not import `crate::lifecycle`;
7. L8E exports remain `pub(crate)`.

Do not add source-guard tests that merely assert documentation links.

## Local Filesystem Evidence

When `localfs` is available, add a focused test:

1. open durable shell on a temporary localfs root;
2. assert writer guard object is the reserved layout object;
3. attempt a second durable shell on the same root and assert lock conflict;
4. drop the first shell;
5. assert the second shell can now assemble;
6. assert the manifest and WAL segment remain valid after reopen.

Gate platform-specific lock semantics appropriately. The test should verify
honest lock behavior without treating platform-specific unsupported semantics
as a contract violation when the backend capabilities say so.

## Sensitivity Probe Ledger

After implementation, record probe results in the L8 porting log with:

1. probe id;
2. mutated file/function;
3. mutation description;
4. test that failed;
5. verification command.

Required probes:

| Probe | Mutation | Expected test family |
|---|---|---|
| E1 | Skip capability preflight. | Capability ordering/generated tests. |
| E2 | Acquire writer lock before preflight. | Backend call-order test. |
| E3 | Hardcode writer-lock object. | Source/layout guard. |
| E4 | Load/create manifest before lock. | Manifest call-order test. |
| E5 | Reject absent manifest instead of create. | New create test. |
| E6 | Replace manifest on existing open. | Existing manifest test. |
| E7 | Ignore database-id mismatch. | Manifest identity test. |
| E8 | Ignore codec mismatch. | Manifest identity test. |
| E9 | Open hardcoded WAL segment. | WAL active-segment test. |
| E10 | Drop writer guard before return. | Guard lifetime/localfs test. |
| E11 | Mark shell open before recovery. | Lifecycle admission test. |
| E12 | Replay WAL during assembly. | No-recovery side-effect test. |
| E13 | Collapse always to standard. | Durability policy test. |
| E14 | Collapse publish uncertainty. | Fault classification test. |
| E15 | Import product/follower/StrataHub vocabulary. | Source guard. |

## Verification Commands

Run after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::durable
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

Optional localfs-specific verification when the environment supports it:

```bash
cargo test -p strata-storage-next --features localfs --locked --lib lifecycle::tests::durable
```

## Exit Criteria

The L8E test suite is complete when it proves:

1. durable-only request validation is enforced;
2. capability preflight happens before writer-lock and durable service work;
3. writer guard ordering and lifetime are pinned;
4. manifest create/load identity and recovery facts are preserved;
5. manifest publish uncertainty is typed;
6. WAL opens the manifest active segment with the correct durability policy;
7. the expected service bundle exists and is crate-private;
8. L6/L7 shells are empty recovery targets;
9. returned durable shell is `Recovering`;
10. ordinary read/commit/maintenance are rejected before recovery completion;
11. no replay, recovery, or maintenance side effect occurs in L8E;
12. generated tests vary input-derived durable assembly paths;
13. source guards prevent product/raw-IO/vocabulary leakage;
14. verification commands pass and the porting log captures evidence.
