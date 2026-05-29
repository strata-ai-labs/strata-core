# Storage-Next Type Proliferation Reduction Plan

Status: draft cleanup plan

## Decision

Do this **operation by operation, grouped by layer/directory**, with a short
inventory pass before each operation-family cleanup.

Do not do this primarily feature by feature. Features are too wide and mix real
boundary types with private scaffolding. Do not do this primarily directory by
directory either. Directory cleanup is useful for sequencing, but by itself it
only redistributes the same type count into smaller files.

The review unit is:

1. one operation family;
2. one owning layer;
3. one behavior-preserving commit when possible;
4. a before/after type and re-export count.

## Goal

Reduce unnecessary struct/enum proliferation in `crates/storage-next` without
weakening storage correctness, recovery evidence, durable format boundaries, or
public API stability.

The problem is not that storage-next uses typed values. Typed values are
appropriate when they cross a layer boundary, validate caller input, preserve a
source error, or record durable/recovery facts. The problem is that too many
private operation steps have grown boundary-shaped type families:

1. `Request`;
2. `Plan`;
3. `Outcome`;
4. `Recovery`;
5. `Candidate`;
6. `PreparedOutput`;
7. operation-local `Kind` / `NoopReason` / `Invalidity`;
8. proof or attestation types for every invariant.

This violates the guidance in
`docs/architecture/storage-next/implementation-patterns.md`: private small
operations should stay small instead of creating a unique type family for each
step.

## Strategy Decision

Use **operation-family cleanup inside a layer**, not pure feature-by-feature or
directory-by-directory cleanup.

### Why Not Feature By Feature

Feature-by-feature cleanup is too broad for this problem. A feature such as
branching, compaction, maintenance, or diagnostics crosses multiple modules and
contains both legitimate boundary types and private scaffolding. Cleaning by
feature risks changing behavior and type boundaries at the same time.

### Why Not Directory By Directory

Directory-by-directory cleanup tends to move types around without reducing
them. For example, splitting `branch/state.rs` is necessary, but a file split by
itself could leave the same `Request`/`Plan`/`Outcome` proliferation in smaller
files.

### Preferred Unit

Clean one **operation family** at a time:

1. identify its public or layer-boundary contract;
2. keep only the types that enforce that contract;
3. inline or merge private one-call-site scaffolding;
4. reduce module re-exports to the remaining boundary surface;
5. run the existing behavior tests before moving to the next operation.

Directories still matter for sequencing. Start in the highest-proliferation
directories, but each cleanup pass should be scoped to one operation family.

## Operating Model

Each cleanup pass follows the same loop:

1. **Inventory**: list the operation's structs, enums, constructors, re-exports,
   call sites, and tests.
2. **Classify**: mark each type as keep, localize, merge, or inline.
3. **Fence behavior**: identify the tests that prove the operation's safety
   contract before editing.
4. **Reduce surface**: remove parent re-exports first, then collapse private
   scaffolding.
5. **Split files only where useful**: split large files to restore ownership,
   not to hide unchanged proliferation.
6. **Verify**: run the targeted unit tests, source guards, format, and clippy.
7. **Record**: update the cleanup ledger with before/after counts and any
   intentionally kept types.

If a pass needs semantic changes, stop and write a normal implementation plan
for that behavior change. This cleanup plan is for type and ownership reduction,
not for changing storage semantics.

## Type Classification

Every type touched by this cleanup should be classified before editing.

### Keep

Keep types when at least one condition is true:

1. Public API or public testkit surface.
2. Durable format, manifest, WAL, snapshot, table-object, or persisted proof.
3. Layer-boundary request or outcome used by multiple modules.
4. Error enum that preserves source errors or recovery-critical facts.
5. Validated configuration or option object reused across callers.
6. Proof token that is passed across a mutation boundary and prevents unsafe
   deletion, pruning, or visibility changes.

### Localize

Move types out of broad module facades when they are legitimate but only local
to one operation:

1. sort keys;
2. staging structs;
3. test fixtures;
4. temporary row collectors;
5. private grouping structs used only inside one implementation file.

These can remain structs, but they should not be re-exported from a parent
module.

### Merge

Merge type families when several private types are just phases of the same
operation and are not independently validated or reused. Common candidates:

1. `Candidate` + `Plan` when candidate selection is immediately consumed;
2. `PreparedOutput` + `Outcome` when prepared output is not reusable;
3. separate proof enums that are always bundled together;
4. operation-local `Recovery` enums that only choose a boolean retry path.

### Inline

Inline types when they are:

1. private;
2. used by one function or one small call chain;
3. not validating invariants;
4. not preserving recovery facts;
5. not named in tests as part of a behavior contract.

## Cleanup Passes

### Pass 0: Inventory And Budget

Add a repeatable inventory command or script under `tools/` or
`scripts/architecture/` that reports:

1. total structs and enums;
2. structs/enums per module;
3. parent-module re-export counts;
4. one-call-site type candidates;
5. files over size thresholds;
6. types with `Request`, `Plan`, `Outcome`, `Recovery`, `Candidate`,
   `PreparedOutput`, `Proof`, `Attestation`, or `Safety` suffixes.

Initial closeout targets:

1. `branch/mod.rs` re-exports fewer than 35 names.
2. `branch/state.rs` is split below 1,500 LOC per file.
3. No `allow(unused_imports)` exists only to preserve speculative scaffolding.
4. Operation families have no more than the boundary types they actually need.
5. Total type count trends down after each pass; no cleanup pass may increase
   net type count without an explicit justification.

The inventory should report both total crate counts and crate-private counts.
Public API and durable-format types should be tracked separately so the cleanup
does not create pressure to delete useful boundary vocabulary.

### Pass 1: Branch Facade And File Split

Scope:

1. `crates/storage-next/src/branch/mod.rs`;
2. `crates/storage-next/src/branch/state.rs`.

Actions:

1. Split `state.rs` by operation:
   - `append`;
   - `rotation`;
   - `fork`;
   - `materialization`;
   - `compaction`;
   - `snapshot_install`;
   - `manifest_recovery`;
   - `local_state`.
2. Keep behavior unchanged.
3. Remove broad re-exports that are only consumed by tests or one sibling
   module.
4. Replace parent-module imports with explicit submodule imports where that
   makes ownership clearer.

Exit gate:

1. No behavior tests change except import paths.
2. `branch/mod.rs` only re-exports real branch-layer boundary types.

This pass is allowed to move code but should not try to collapse compaction,
materialization, or snapshot-install types yet. Those are separate operation
families.

### Pass 2: Branch Compaction Family

Current smell:

Branch compaction has a broad family of types for one operation:
`Request`, `Plan`, `Outcome`, `Recovery`, `Candidate`, `PreparedOutput`,
`Kind`, `NoopReason`, `Invalidity`, `PruningPolicy`, `PruningProof`,
`RetentionPolicy`.

Actions:

1. Keep the external request/outcome only if lifecycle callers need them.
2. Inline candidate and prepared-output structs if they are consumed in one
   path.
3. Collapse no-op and invalidity enums when one error enum or outcome reason is
   enough.
4. Replace multiple pruning/proof parameters with one aggregate proof if the
   operation always consumes them together.

Exit gate:

1. Compaction behavior tests pass unchanged.
2. Unsafe pruning remains proof-gated.
3. Fewer compaction-specific types are exported from `branch`.

### Pass 3: Branch Materialization And Snapshot Install

Scope:

1. materialization request/intent/handle/outcome/recovery/prepared-output;
2. snapshot install request/group/outcome/recovery/policy types.

Actions:

1. Keep stable handles if they protect against layer-index drift.
2. Merge request and intent if both exist only to stage one executor call.
3. Merge branch-level and group-level snapshot outcomes unless callers need
   both.
4. Localize staging structs inside operation modules.

Exit gate:

1. Handle-based materialization safety remains tested.
2. Snapshot recovery tests still distinguish missing, corrupt, and policy
   outcomes.

### Pass 4: Branch Facts, Read, And Pruning Proofs

Scope:

1. `branch/facts.rs`;
2. `branch/read.rs`;
3. `branch/pruning.rs`.

Actions:

1. Keep read-bound and visible-row types that cross API/lifecycle boundaries.
2. Localize sort keys, observed facts, and candidate row helpers.
3. Merge proof micro-types when they are always consumed as one pruning
   capability bundle.
4. Remove parent re-exports for facts that only support one lifecycle module.

Exit gate:

1. Branch read and history tests pass.
2. Public API/L9 does not import branch internals directly.

### Pass 5: Lifecycle Maintenance, Checkpoint, And Flush

Scope:

1. `lifecycle/maintenance.rs`;
2. `lifecycle/checkpoint.rs`;
3. `lifecycle/flush.rs`;
4. durable maintenance adapters.

Actions:

1. Keep task request/outcome types at the executor boundary.
2. Inline private runner-specific status wrappers where they only map one
   lower-layer outcome.
3. Merge health-debt and source-error fields into shared outcome helpers rather
   than unique outcome structs per operation.
4. Remove dead task/fault/status variants that were created for future slices
   but are now superseded.

Exit gate:

1. Maintenance queue semantics remain deterministic.
2. Checkpoint success and WAL-truncation debt remain distinguishable.
3. Flush orphan and uncertainty facts remain typed.

### Pass 6: Lifecycle Retention, Quarantine, Rewrite, And Budget

Scope:

1. retention proof and pruning surfaces;
2. quarantine/purge/repair operation families;
3. rewrite publication outputs;
4. memory/budget facts.

Actions:

1. Keep proof tokens that bind inventory generations or prevent unsafe
   reclamation.
2. Merge duplicate proof/context/report structs when they carry the same
   object names and epochs.
3. Inline private policy structs that are only constant knobs.
4. Preserve recovery-health and source-error fidelity.

Exit gate:

1. Table-object retention still fails closed.
2. Quarantine inventory mismatch still blocks unsafe purge/reclaim.
3. Budget admission tests still prove bounded allocation.

### Pass 7: Service Layer

Scope:

1. manifest services;
2. table services;
3. WAL services;
4. quarantine services;
5. snapshot services.

Actions:

1. Keep service errors and persisted format wrappers.
2. Merge write/load wrappers that exist only to expose metadata already
   available from backend outcomes.
3. Localize mutation/reconcile private stage structs.
4. Avoid leaking service-specific helper types into lifecycle module facades.

Exit gate:

1. Durable publication fault-window tests pass.
2. Format golden vectors remain unchanged.

### Pass 8: API And Testkit Surface

Scope:

1. `api/*`;
2. `testkit/*`.

Actions:

1. Keep public API shells even if numerous; they are the L9 boundary.
2. Remove internal-only public-looking constructors or reports that are not
   consumed by engine-next.
3. Collapse testkit result counters when they do not represent distinct
   behavior.
4. Keep source guards for no lower-layer type leakage.

Exit gate:

1. Public API conformance tests pass.
2. Source guards still reject lower-layer type leaks.

Public API type count is not automatically bad. The cleanup target here is
internal duplication, misleading constructors, and testkit counter inflation,
not shrinking a stable consumer-facing vocabulary just to improve a metric.

### Pass 9: Closeout Guards

Add cleanup guards that prevent re-growth:

1. parent-module re-export count guard for high-risk modules;
2. max operation-family suffix count per module;
3. no new `allow(unused_imports)` for speculative scaffold exports;
4. no new `FooArgs` / `FooInput` / `FooResult` type names unless justified;
5. type inventory snapshot recorded in this cleanup directory.

## Execution Rules

1. One operation family per commit.
2. No physical format changes in this cleanup.
3. No public API removal unless L9 documents the replacement.
4. No error-code changes unless the existing code is demonstrably wrong.
5. No behavior changes hidden inside file splits.
6. Every removed type must be either inlined, merged, or localized.
7. Every kept proof type must state the invariant it protects.
8. Every kept request/plan/outcome type must identify the layer boundary it
   crosses.

## Recommended Order

1. Inventory and type-budget script.
2. Branch file split and facade reduction.
3. Branch compaction family reduction.
4. Branch materialization and snapshot-install reduction.
5. Branch facts/read/pruning proof reduction.
6. Lifecycle maintenance/checkpoint/flush reduction.
7. Lifecycle retention/quarantine/rewrite/budget reduction.
8. Service layer reduction.
9. API/testkit cleanup.
10. Source guards and closeout.

This order starts where the proliferation is most visible while keeping each
pass small enough to review. It also avoids the highest-risk mistake: moving
hundreds of types by directory without proving that any of them were actually
unnecessary.

## First Backlog

These are the first concrete cleanup units. They are intentionally smaller than
the full passes above.

| Order | Unit | Primary files | Primary action | Expected result |
|---|---|---|---|---|
| 1 | Inventory tool | `crates/storage-next/src/**` | Count types, suffix families, re-exports, one-call-site candidates | Baseline before cleanup |
| 2 | Branch facade | `branch/mod.rs` | Remove broad re-exports and update imports | Smaller branch public-in-crate surface |
| 3 | Branch state split | `branch/state.rs` | Move operation groups into owned files without semantic changes | Reviewable ownership boundaries |
| 4 | Branch compaction | `branch/state/*`, `branch/pruning.rs` | Collapse private candidate/prepared/recovery scaffolding | Fewer compaction-only types |
| 5 | Branch materialization | `branch/state/*` | Keep handles, merge private request/intent wrappers where safe | No layer-index safety regression |
| 6 | Snapshot install/recovery | `branch/state/*` | Merge local group/outcome/recovery wrappers where callers do not need all layers | Smaller recovery family |
| 7 | Branch proof bundle | `branch/pruning.rs`, `branch/facts.rs` | Consolidate micro-proofs consumed together | Fewer proof/attestation types |
| 8 | Maintenance executor | `lifecycle/maintenance.rs` | Keep task boundary, merge runner-only status wrappers | Less operation-family duplication |
| 9 | Retention/quarantine/rewrite | `lifecycle/retention.rs`, `lifecycle/quarantine.rs`, `lifecycle/rewrite.rs` | Bind real proof tokens, merge duplicate reports | Less duplicated health/object fact plumbing |
| 10 | API/testkit reports | `api/*`, `testkit/*` | Keep public types, reduce duplicate counters and dead shells | Clearer API diagnostics surface |

## Per-Operation Checklist

Before changing an operation family, answer these questions in the commit
message or cleanup ledger:

1. Which type is the actual boundary type for the operation?
2. Which types are only staging names inside one call chain?
3. Which tests prove the operation's safety invariant?
4. Which proof types prevent unsafe deletion, pruning, publication, or
   visibility changes?
5. Which re-exports are still necessary after call sites use explicit modules?
6. Did total type count, operation-family type count, and parent re-export
   count go down?

## Keep/Remove Examples

Examples that usually stay:

1. `Storage*Request`, `Storage*Summary`, and `StorageApiError` public API
   types.
2. Durable manifest, WAL, snapshot, table-object, and golden-vector format
   types.
3. Error enums carrying source errors, object names, branch IDs, commit
   versions, or publish windows.
4. Handle/proof tokens that protect against stale layer indices, stale
   inventory, stale retention proof, or unsafe reclaim.

Examples that should be challenged:

1. A `Candidate` immediately converted into a `Plan` in the same function.
2. A `PreparedOutput` that is never retried or inspected independently.
3. A `Recovery` enum that only maps to retry true/false in one match.
4. A `NoopReason` enum consumed by one test and one debug string.
5. An `Attestation` type that is always embedded in another proof and never
   travels alone.
6. A parent-module re-export used only to make tests shorter.
