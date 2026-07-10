# L8E Implementation Plan: Durable Open/Create Service Assembly

Status: implemented

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-test-plan.md`

## Objective

Implement the durable-local lifecycle assembly step.

L8E turns an accepted durable-local open plan into a storage-owned durable
service shell by acquiring the backend writer guard, loading or creating the
database manifest, opening the active WAL segment, constructing the L4 durable
service bundle, and preparing empty L6/L7 recovery targets. It must preserve raw
facts for L8F/L8G without replaying WAL records, installing recovered rows,
starting maintenance, or exposing the runtime as open for ordinary reads and
commits.

This slice is intentionally a boundary slice. L8E may perform durable
open/create side effects needed to establish the database root and active WAL
segment. Recovery orchestration starts in L8F. L7 replay/bootstrap starts in
L8G. Maintenance, flush, checkpoint, retention, quarantine, repair, and full
durable close land in later L8 slices.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L8/l8c-storage-mode-capability-validation-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-implementation-plan.md`
8. `crates/storage-next/src/lifecycle/`
9. `crates/storage-next/src/backend/mod.rs`
10. `crates/storage-next/src/backend/local_fs.rs`
11. `crates/storage-next/src/layout/mod.rs`
12. `crates/storage-next/src/format/manifest.rs`
13. `crates/storage-next/src/service/manifest.rs`
14. `crates/storage-next/src/service/wal.rs`
15. `crates/storage-next/src/service/sidecar.rs`
16. `crates/storage-next/src/service/snapshot.rs`
17. `crates/storage-next/src/service/table.rs`
18. `crates/storage-next/src/service/checkpoint.rs`
19. `crates/storage-next/src/service/quarantine.rs`
20. `crates/storage-next/src/branch/`
21. `crates/storage-next/src/commit/`
22. `crates/engine/src/database/open.rs`
23. `crates/engine/src/database/recovery.rs`
24. `crates/engine/src/database/lifecycle.rs`

## Existing-Code Source Map

| Current file | L8E evidence | L8E action |
|---|---|---|
| `crates/engine/src/database/open.rs` | Old primary open acquires the process/file guard, runs recovery, creates the WAL writer, then publishes the opened database. | Port only the durable assembly ordering. Product registry, config files, public open wording, subsystem recovery, and background threads stay above or after L8E. |
| `crates/storage/src/durability/recovery_bootstrap.rs` | Old durable bootstrap prepares MANIFEST/codec state before replay. | Preserve the storage rule: manifest identity and codec facts are established before recovery reads snapshots or WAL records. |
| `crates/storage-next/src/lifecycle/capability.rs` | L8C already validates durable-local capabilities and preserves `DurabilityPolicy`. | L8E must call this preflight before writer-lock acquisition or manifest/WAL side effects. |
| `crates/storage-next/src/backend/mod.rs` | `BackendWriterGuard` is the RAII writer-lock lifetime; `Backend::acquire_writer_lock` is the only lower-layer lock surface. | L8E owns the guard in the durable assembly shell and releases it only through later close/drop semantics. |
| `crates/storage-next/src/layout/mod.rs` | `ObjectLayout::writer_lock()` owns the reserved writer-lock object name. | L8E must request that exact object; no string literal lock path is allowed in lifecycle code. |
| `crates/storage-next/src/service/manifest.rs` | `DatabaseManifestService` can load the current manifest or create the initial manifest through durable publish. | L8E composes this service and maps missing manifest to create; it does not encode manifest bytes itself. |
| `crates/storage-next/src/format/manifest.rs` | `DatabaseManifest` carries database id, codec id, active WAL segment, snapshot watermark, snapshot id, and flush watermark. | L8E validates the manifest identity against the open request and carries recovery facts forward unmodified. |
| `crates/storage-next/src/service/wal.rs` | `WalService::open` validates config/capabilities and opens or creates the active segment. | L8E opens WAL only after manifest identity is known and while the writer guard is held. |
| `crates/storage-next/src/service/sidecar.rs` | WAL segment sidecars are optional accelerators over segment metadata. | L8E constructs the sidecar service, but L8F owns loading/reconciliation. |
| `crates/storage-next/src/service/snapshot.rs` | Snapshot service loads and publishes row-native snapshots. | L8E constructs the service only; L8F/L8J own reads and writes. |
| `crates/storage-next/src/service/table.rs` | Table object services publish/read immutable tables. | L8E constructs the services only; L8F/L8I/L8K own table recovery/publication. |
| `crates/storage-next/src/service/checkpoint.rs` | Checkpoint service composes manifest and snapshot services. | L8E can include it in the service bundle, but checkpoint execution is later. |
| `crates/storage-next/src/service/quarantine.rs` | Quarantine service loads/publishes inventory and mutates quarantine objects. | L8E constructs the service only; L8F/L8M own reconciliation and mutation. |
| `crates/storage-next/src/branch/state.rs` | L6 branch state is the target for recovered rows. | L8E may prepare empty branch shells, but must not install recovered rows. |
| `crates/storage-next/src/commit/` | L7 allocators, visible tracker, durable gate, and replay hooks exist. | L8E may prepare empty L7 shells, but L8G owns allocator catch-up and visibility publication from recovered facts. |

## Scope

L8E implements:

1. a crate-private durable lifecycle assembly module;
2. a durable open/create request carrying storage mode, database id, initial
   branch id/generation, branch runtime config, commit runtime config, and WAL
   service config, with the timestamp source supplied when constructing the
   shell;
3. validation that accepts only `StorageMode::DurableLocalStandard` and
   `StorageMode::DurableLocalAlways`;
4. capability preflight through L8C before any durable side effect;
5. writer-lock object construction through `ObjectLayout::writer_lock()`;
6. backend writer-lock acquisition after capability preflight and before
   manifest or WAL mutation;
7. database manifest load for existing databases;
8. initial database manifest create when no manifest exists;
9. manifest identity validation against requested database id and codec id;
10. create-race handling for a manifest that appears between load and create;
11. manifest publish uncertainty classification with raw lower-layer source
    preserved;
12. WAL service open using the manifest active WAL segment and requested
    durability policy;
13. assembly of durable L4 service handles for database manifest, table
    manifest, WAL, WAL sidecar, snapshot, table object write/read, checkpoint,
    and quarantine;
14. empty L6 branch recovery targets and empty L7 commit-runtime shells for
    later replay/bootstrap;
15. a durable assembly outcome/fact shape that records mode, disposition,
    database id, codec id, active WAL segment, durability policy, writer-lock
    object, manifest recovery facts, and service readiness;
16. lifecycle state transition from `New` to `Opening` to `Recovering`;
17. no ordinary read/commit admission from the durable shell before L8F/L8G
    complete recovery;
18. generated/testkit counters for durable standard assembly, durable always
    assembly, manifest create, manifest existing load, writer-lock behavior,
    WAL service open, no replay, and no maintenance;
19. an L8E porting-log entry after implementation.

L8E does not implement:

1. public L9 open/read/commit APIs;
2. object-durable candidate production open;
3. follower or read-only open;
4. product registry, primitive reconstruction, IPC, engine freeze hooks, or
   StrataHub behavior;
5. WAL replay;
6. L7 allocator catch-up from recovered commits;
7. recovered visible-version publication;
8. snapshot/table/quarantine recovery orchestration;
9. partial-tail repair policy;
10. checkpoint execution;
11. maintenance scheduling;
12. flush, compaction, materialization, retention, reclaim, purge, or repair;
13. close drain, close sync, or explicit writer-lock release beyond RAII
    ownership in the shell;
14. background WAL sync thread or task executor.

## Design Decisions

### Durable Assembly Is Not Recovery

L8E establishes the durable services that recovery will use. It must not treat
service construction as evidence that the database is recovered.

Rules:

1. The returned durable shell remains in `LifecycleState::Recovering`.
2. Ordinary reads, commits, and ordinary maintenance are rejected by lifecycle
   admission.
3. No `StorageOpenOutcome` with final `Open` semantics is produced yet.
4. The shell carries assembly facts for L8F/L8G to finish recovery and produce
   the final open outcome.
5. Empty/new durable databases still flow through the recovery/bootstrap slices
   so all durable opens share one completion path.
6. The parent-plan phrase "reports service facts in `StorageOpenOutcome`" is
   implemented in two stages: L8E records service assembly facts, and L8G owns
   the final `StorageOpenOutcome` once recovery facts are trusted. If
   `StorageOpenOutcome` grows service-fact fields later, they must be populated
   from the L8E assembly facts rather than recomputed.

### Open/Create Manifest Policy

L8E should use the database manifest as the durable root.

Suggested sequence:

```text
validate request
transition New -> Opening
validate backend capabilities through L8C
build writer-lock object through ObjectLayout
acquire backend writer guard
construct DatabaseManifestService
load_current()
if manifest exists:
  validate database_id, codec_id, active_wal_segment, and recovery facts
  disposition = OpenedExisting
else:
  create_initial(database_id, codec_id)
  disposition = Created
if create_initial returns precondition/visibility race:
  reload manifest, validate identity, classify as OpenedExisting only if exact
```

The implementation must not build manifest object names by hand or encode
manifest bytes in lifecycle. Manifest load/create must go through
`DatabaseManifestService`.

If create returns `VisibleDurabilityUnconfirmed` or `VisibilityUnknown`, the
error must preserve the `PublishError` source and record a durable assembly
fact that the manifest create outcome is uncertain. L8F/L8N can later decide
whether retry/recovery is safe.

### Writer Guard Ordering And Lifetime

The writer guard is the first durable side effect after capability preflight.

Rules:

1. Missing capabilities reject before `ObjectLayout::writer_lock()` is needed.
2. Writer-lock layout failure maps to `LifecycleLowerLayer::Layout`.
3. Writer-lock backend failure maps to `LifecycleLowerLayer::Backend`.
4. No manifest load/create or WAL open happens if the writer guard is not held.
5. The durable shell owns `BackendWriterGuard`.
6. The guard must not be dropped before the shell is dropped or explicitly
   closed by a later L8N path.
7. L8E must not expose the guard publicly.

### Durable Standard And Durable Always Share Assembly

The service set is the same for durable standard and durable always. The
difference is preserved as a `DurabilityPolicy` fact and passed into
`WalService::open`.

Rules:

1. `StorageMode::DurableLocalStandard` maps to `DurabilityPolicy::Standard`.
2. `StorageMode::DurableLocalAlways` maps to `DurabilityPolicy::Always`.
3. The accepted capability outcome must still identify the original lifecycle
   mode.
4. L8E must not collapse durable always into standard in the returned facts.
5. Per-commit force-durability behavior remains in L7 durable commit execution,
   not in service assembly.

### L4 Services Are Internal Assembly Facts

L8E may own L4 service handles, but it must not leak them to L9/engine.

Expected services:

1. `DatabaseManifestService`;
2. `TableManifestService`;
3. `WalService`;
4. `WalSegmentMetadataSidecarService`;
5. `SnapshotService`;
6. `TableObjectService`;
7. `TableObjectReaderService`;
8. `CheckpointService`;
9. `QuarantineService`.

Service construction should be centralized in one bundle so later recovery,
maintenance, and close paths cannot accidentally create a second WAL or
manifest service with different facts.

### L6/L7 Shells Are Recovery Targets

L8E prepares the empty L6/L7 objects that later recovery will populate.

Rules:

1. Branch state starts empty.
2. Branch registry contains the configured initial branch/generation only if
   the implementation needs a concrete root branch before recovery.
3. Visible version starts at `CommitVersion::ZERO`.
4. Commit allocator starts at zero.
5. Timestamp guard starts with no allocated timestamp.
6. Unresolved durable gate starts empty.
7. L8G, not L8E, catches these facts up from recovered WAL/timeline facts.

### Source-Chain Preservation

L8E sits at an orchestration boundary, so lower-layer failures must not collapse
into unstructured lifecycle messages.

Mapping rules:

1. backend capability and writer-lock failures preserve `BackendError`;
2. layout failures preserve `LayoutError`;
3. manifest failures preserve `ManifestServiceError`;
4. WAL open failures preserve `WalServiceError`;
5. branch-shell failures preserve L6 branch errors;
6. commit-shell failures preserve L7 commit errors;
7. display text remains storage-shaped and product-neutral.

## Module Layout

Add a focused durable assembly module:

```text
crates/storage-next/src/lifecycle/
  durable.rs
```

Update `mod.rs` to crate-private re-export the L8E surface.

Tests should stay split:

```text
crates/storage-next/src/lifecycle/tests/
  durable.rs
```

Expected ownership after L8E:

1. `capability.rs`: side-effect-free storage-mode capability preflight.
2. `cache.rs`: cache open/commit/read/close baseline runtime.
3. `durable.rs`: durable local service assembly and recovery shell.
4. `tests/durable.rs`: direct durable assembly tests.
5. `testkit/lifecycle/durable.rs`: generated durable assembly scripts and
   counters.

## Proposed Type Surface

Names may change if responsibilities remain intact. All production items stay
`pub(crate)`.

### `LifecycleDurableLocalOpenRequest`

Suggested shape:

```text
LifecycleDurableLocalOpenRequest {
  plan: StorageOpenPlan,
  database_id: [u8; 16],
  initial_branch_id: BranchId,
  branch_generation: CommitBranchGeneration,
  branch_config: BranchRuntimeConfig,
  commit_config: CommitRuntimeConfig,
  wal_config: WalServiceConfig,
}
```

Rules:

1. `plan.storage_mode()` must be `DurableLocalStandard` or
   `DurableLocalAlways`;
2. `plan.codec_id()` is the manifest/WAL codec identity;
3. `database_id` is validated against an existing manifest when present;
4. branch generation must be nonzero through the existing L7 type;
5. configs validate before service mutation when validation is side-effect-free;
6. requested codec id must be compatible with `wal_config.codec_id()` before
   manifest create, so an unsupported V1 WAL codec cannot leave behind a newly
   published manifest;
7. request validation must not inspect backend objects.

### `LifecycleDurableAssemblyFacts`

Suggested fields:

```text
LifecycleDurableAssemblyFacts {
  mode: StorageMode,
  disposition: StorageOpenDisposition,
  database_id: [u8; 16],
  codec_id: LifecycleCodecId,
  durability_policy: DurabilityPolicy,
  active_wal_segment: u64,
  writer_lock_object: ObjectName,
  manifest_snapshot_watermark: Option<u64>,
  manifest_snapshot_id: Option<u64>,
  manifest_flush_watermark: Option<CommitVersion>,
}
```

Rules:

1. facts must be derived from the manifest and capability outcome;
2. no field is inferred from object listing;
3. no product branch name or primitive registry fact appears here;
4. facts are available to L8F/L8G and tests without exposing service handles.

### `LifecycleDurableLocalServices<'a>`

Suggested shape:

```text
LifecycleDurableLocalServices<'a> {
  backend: &'a dyn Backend,
  writer_guard: BackendWriterGuard,
  capability_outcome: LifecycleCapabilityOutcome,
  manifest: DatabaseManifestService<'a>,
  table_manifest: TableManifestService<'a>,
  wal: WalService<'a>,
  wal_sidecar: WalSegmentMetadataSidecarService<'a>,
  snapshot: SnapshotService<'a>,
  table_object: TableObjectService<'a>,
  table_reader: TableObjectReaderService<'a>,
  checkpoint: CheckpointService<'a>,
  quarantine: QuarantineService<'a>,
  assembly_facts: LifecycleDurableAssemblyFacts,
}
```

Rules:

1. the bundle borrows the backend and owns the writer guard;
2. service constructors receive the same backend reference;
3. `WalService` is opened exactly once for the manifest active segment;
4. service handles are crate-private;
5. tests may inspect facts, not mutable service internals.

### `LifecycleDurableLocalShell<'a, S>`

Suggested shape:

```text
LifecycleDurableLocalShell<'a, S> {
  state: LifecycleStateMachine,
  open_plan: StorageOpenPlan,
  services: LifecycleDurableLocalServices<'a>,
  branch_targets: ...,
  commit_targets: ...,
  timestamp_source: S,
}
```

Rules:

1. returned state is `Recovering`;
2. commit/read methods are absent or reject through lifecycle admission;
3. L8F/L8G receive the shell and return the final opened durable runtime later;
4. closing a shell before recovery completion drops the writer guard by RAII
   and returns a typed close/interrupted-open fact if L8N exposes that path.

## Durable Assembly Algorithm

Suggested high-level implementation:

```text
fn assemble_durable_local(request, backend, timestamp_source):
  request.validate()
  state = LifecycleStateMachine::new()
  admit Open
  transition OpenRequested

  capability = validate_backend_capabilities_for_open(request.plan, backend)
  policy = capability.durability_policy().expect("durable policy")

  lock_object = ObjectLayout::writer_lock()
  writer_guard = backend.acquire_writer_lock(&lock_object)

  manifest_service = DatabaseManifestService::new(backend)
  manifest_load = manifest_service.load_current()
  manifest, disposition = load_or_create_manifest(manifest_service, request)
  validate_manifest_identity(manifest, request)

  wal = WalService::open(
    backend,
    request.database_id,
    manifest.active_wal_segment(),
    policy,
    request.wal_config,
  )

  services = construct_service_bundle(...)
  branch_targets = construct_empty_branch_targets(...)
  commit_targets = construct_empty_commit_targets(...)

  transition DurableRecoveryRequired
  return LifecycleDurableLocalShell { state: Recovering, ... }
```

Implementation notes:

1. `load_or_create_manifest` should be a small helper with direct tests.
2. Manifest create should use `DatabaseManifestService::create_initial`.
3. Existing manifest load should use `load_current` and explicit identity
   validation, because the lifecycle request owns both database id and codec id.
4. WAL open should use `WalServiceConfig::validate`.
5. A WAL open failure after manifest create remains a typed partial durable
   assembly failure; the manifest may be visible and recovery must be able to
   retry.
6. No recovery health is finalized here.

## Error Handling

Required typed error categories:

1. invalid non-durable plan;
2. capability mismatch;
3. writer-lock layout failure;
4. writer-lock unavailable;
5. manifest read failure;
6. manifest decode/corruption;
7. manifest database-id mismatch;
8. manifest codec mismatch;
9. manifest create precondition race;
10. manifest create failed before visibility;
11. manifest create visibility unknown;
12. manifest create visible but durability unconfirmed;
13. WAL config invalid;
14. WAL open/create failure;
15. branch shell construction failure;
16. commit shell construction failure.

Error display must not contain engine `Database::open`, product open policy,
primitive registry, follower, IPC, or StrataHub vocabulary.

## Source Guard Policy

L8E production code may import:

1. `crate::backend`;
2. `crate::layout`;
3. `crate::format::DatabaseManifest` facts only if needed for identity checks;
4. `crate::service`;
5. `crate::branch`;
6. `crate::commit`;
7. `crate::lifecycle`;
8. `crate::config::mode::DurabilityPolicy`;
9. `strata_core_next` storage fact types.

L8E production code must not import:

1. `crates/engine` or product modules;
2. raw `std::fs`, `std::path::Path`, or `std::env`;
3. follower, IPC, primitive registry, search, graph, vector, inference, or
   StrataHub vocabulary;
4. public L9 API wrappers;
5. object-durable production fencing code beyond capability facts.

## Generated/Property Coverage

Extend `src/testkit/lifecycle/` with durable assembly counters:

1. durable standard accepted;
2. durable always accepted;
3. cache/object candidate rejected by durable assembly;
4. missing capability rejects before lock;
5. writer lock acquired;
6. writer lock failure rejects before manifest;
7. absent manifest creates initial manifest;
8. existing manifest opens existing;
9. manifest codec/database mismatch rejects;
10. manifest publish uncertainty recorded;
11. WAL open uses manifest active segment;
12. no WAL replay;
13. no snapshot/table/quarantine reads during assembly;
14. returned shell is `Recovering`;
15. ordinary read/commit rejected before recovery completion.

The generated contract should vary storage mode, capability subsets, manifest
presence, manifest identity, active WAL segment, durability policy, and injected
failure points. It must not satisfy every counter through a fixed canonical
script before consuming generated bytes.

## Sensitivity Probes

Record the following probes in the L8 porting log after implementation:

| Probe | Mutation | Expected failure |
|---|---|---|
| E1 | Skip L8C capability preflight. | Capability-ordering test or generated counter fails. |
| E2 | Acquire writer lock before capability preflight. | Backend call-order test fails. |
| E3 | Use a hardcoded lock literal instead of `ObjectLayout::writer_lock()`. | Source/layout guard fails. |
| E4 | Load/create manifest before writer guard. | Durable call-order test fails. |
| E5 | Treat missing manifest as recovery failure instead of create. | New-database create test fails. |
| E6 | Treat existing manifest as create. | Existing-open test or create-precondition test fails. |
| E7 | Ignore manifest database-id mismatch. | Manifest identity test fails. |
| E8 | Ignore manifest codec mismatch. | Manifest identity test fails. |
| E9 | Open WAL segment `1` instead of manifest active segment. | Active-segment test fails. |
| E10 | Drop writer guard before returning the shell. | Dual-opener/guard-held test fails. |
| E11 | Mark durable shell `Open` before recovery. | Admission/state test fails. |
| E12 | Replay WAL during assembly. | No-replay side-effect test fails. |
| E13 | Construct only standard policy for durable always. | Durability policy preservation test fails. |
| E14 | Collapse manifest publish uncertainty into generic failure. | Source-chain/classification test fails. |
| E15 | Import product/engine/follower/StrataHub vocabulary. | Source guard fails. |

## Verification Commands

Run after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::durable
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo test -p strata-storage-next --all-features --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## Exit Criteria

L8E is complete when:

1. durable standard and durable always assembly both work;
2. missing capabilities reject before durable side effects;
3. writer guard is acquired before manifest/WAL mutation and held in the shell;
4. missing manifest creates an initial durable manifest;
5. existing manifest loads and preserves recovery facts;
6. manifest identity mismatches fail closed;
7. WAL opens on the manifest active segment with the correct durability policy;
8. every expected durable service is assembled once from the same backend;
9. the returned durable shell is in `Recovering`, not `Open`;
10. ordinary reads/commits are unavailable until L8F/L8G complete recovery;
11. lower-layer source chains are preserved;
12. source guards prevent product, raw filesystem, follower, and public API
    leakage;
13. the L8 porting log records shipped files, intentional changes, deferred
    items, sensitivity probes, and verification commands.
