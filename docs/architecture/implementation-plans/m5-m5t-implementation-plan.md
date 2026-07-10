# M5 / M5T Implementation Plan: Engine-Next Persistence Adapter And Control Plane

Status: draft implementation plan

## Goal

Create the engine-next persistence boundary, minimal control plane, and first
end-to-end product spine: cache and durable database open, branch creation, and
KV put/get/delete through storage-next L9.

This milestone is not a broad product rewrite. It deliberately implements only
the branch and KV surface needed to prove that engine-next can consume
storage-next correctly without direct storage imports outside the persistence
adapter.

## Inputs

1. `docs/architecture/engine-architecture.md`
2. `docs/architecture/engine/target-crate-shape-and-test-harness.md`
3. `docs/architecture/engine/persistence-adapter-contract.md`
4. `docs/architecture/engine/control-plane-layout-contract.md`
5. `docs/architecture/engine/storage-space-id-registry.md`
6. `docs/architecture/runtime-resource-profile-architecture.md`
7. `docs/architecture/intelligence-architecture.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M5A` | Engine crate skeleton | Create engine-next with target module buckets, crate-level policy, feature gates, and no old subsystem architecture. | Crate builds with persistence and control modules empty or stubbed. |
| `M5B` | Persistence adapter | Implement the only normal engine path to storage-next L9. | Dependency guards prove storage imports are isolated to persistence. |
| `M5C` | Physical row encoding | Implement storage-space ID routing and product-reference-to-row-key encoding. | Engine owns all product reference encoding; storage sees opaque row keys. |
| `M5D` | Control-plane layout | Implement `_system_` branch and branch-local `_system_` space bootstrap and validation. | Registry rows are created, validated, and fail closed when corrupt. |
| `M5E` | Runtime resource profile | Resolve host facts and user config into storage, engine, derived-state, and inference hints, then pass storage budgets through the M4 L9 constructor/config boundary. | Resolved budgets are passed downward without global state. |
| `M5F` | Error mapping | Map storage diagnostics into executor-facing engine errors while preserving source chains. | Engine errors do not expose storage enum names as executor API. |
| `M5G` | Branch and KV vertical spine | Implement the first product-capability path over the persistence adapter: open cache/local, bootstrap control rows, create branches, and put/get/delete KV. | Cache and durable-local tests prove branch and KV behavior end to end without storage-next imports outside persistence. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M5TA` | Dependency guards | Scan crate graph and source imports. | No engine module outside persistence imports storage-next. |
| `M5TB` | Persistence fake tests | Test engine behavior against fake L9 storage outcomes. | Adapter handles success, corruption, unavailable backend, and ambiguous commit facts. |
| `M5TC` | Control-plane bootstrap tests | Cover new database, matching registry, missing rows, corrupt rows, and version mismatch. | Failures use stable error codes. |
| `M5TD` | Resource profile tests | Exercise edge, desktop, server, unknown, and explicit profiles. | Budgets are deterministic and explainable. |
| `M5TE` | Error mapping tests | Validate class, code, source, redaction, and retry behavior. | Engine errors obey the global diagnostics contract. |
| `M5TF` | Branch and KV conformance | Run cache and durable-local branch/KV workflows against the executor-facing engine-next API. | Open, branch create, KV mutation/read/delete, branch isolation, and durable reopen all pass through persistence only. |

## Convergence Notes

1. `M5TA` lands with `M5B` and remains active through all later engine work.
2. `M5TB` lands before product capability work starts.
3. `M5TC` lands with `M5D`.
4. `M5TD` lands with `M5E`.
5. `M5D` and `M5E` begin the engine surfaces required by intelligence-next
   "Engine Surface Consumed"; M6 completes the product-facing parts.
6. `M5G` is the first vertical proof and may land as soon as the minimal parts
   of `M5A`, `M5B`, `M5C`, `M5D`, and `M5F` exist. It must not wait for JSON,
   event, vector, graph, retrieval, IPC, clone, or full branch workflow work.

## Slice Policy

M5 may implement only the branch and KV behavior described in `M5G`. JSON,
event, vector, graph, retrieval, IPC, clone, branch merge/diff/restore, and
cross-capability orchestration remain out of scope.

The branch and KV code must use permanent product-domain names. Planning labels
such as `M5G` are documentation metadata only and must not appear in production
module names, type names, function names, errors, metrics, tests, or comments.

## First Vertical Slice: Branch And KV

Detailed slice plans:

1. `docs/architecture/implementation-plans/m5g-branch-kv-vertical-spine-implementation-plan.md`
2. `docs/architecture/implementation-plans/m5g-branch-kv-vertical-spine-test-plan.md`

### Scope

1. Create `crates/engine-next` with the target module buckets needed for the
   vertical slice.
2. Open a new cache database.
3. Open a new durable-local database.
4. Bootstrap minimal control-plane rows for database identity, storage-space
   registry version, branch catalog, default branch, and KV capability support.
5. Create product branches from an explicit source branch head.
6. Put, get, and delete KV records on a branch.
7. Reopen durable-local storage and observe the same branch catalog and KV
   rows.
8. Prove storage-next imports are isolated to `persistence`.

### Binding Decisions

1. **The first product spine is branch plus KV only.**
   KV is implemented because it is the smallest product capability over the
   branch-aware MVCC row substrate. Other capabilities must not be scaffolded
   into the first slice beyond empty module placeholders required by crate
   shape.
2. **Every executor-facing write is one internal commit plan.**
   Engine-next must not recreate public manual transaction sessions. A KV put,
   delete, or batch is translated into an internal commit plan and submitted
   through persistence.
3. **Persistence is the only storage-facing module.**
   `api`, `runtime`, `branch`, `data`, `control`, `diagnostics`, and `config`
   use engine-owned abstractions. Only `persistence` imports storage-next L9.
4. **Branch names are engine product state.**
   Storage-next receives opaque `BranchId` and generation facts. Engine-next
   owns branch names, reserved-name validation, branch catalog rows, and the
   mapping from product name to storage branch identity.
5. **Branch creation is not treated as a single storage call.**
   The product branch exists only when the engine branch catalog records it as
   active. The implementation must account for the storage-branch operation and
   control-plane catalog update as a failure window, either by making the open
   path repair/complete/tombstone pending branch records or by failing closed
   with a stable diagnostic.
6. **KV rows use the registry assignment.**
   KV source rows use storage-space ID `0x20`. User data lives in user product
   spaces. Control-plane rows live under the control IDs from the storage-space
   registry.
7. **Cache and durable-local differ only by durability.**
   Cache mode must exercise the same engine branch and KV semantics as durable
   mode. Durable-local adds reopen and sync proof; it must not silently fall
   back to cache.
8. **The system branch and system space are hidden product axes.**
   `_system_` branch and `_system_` space are used for engine control rows and
   must not be returned by ordinary branch or space listing APIs.

### Implementation Order

1. **Crate Skeleton**
   - Add `crates/engine-next/Cargo.toml` and wire it into the workspace.
   - Add crate policy in `src/lib.rs`: unsafe denied, permanent modules only,
     and a small intentional executor-facing re-export surface.
   - Create the slice modules:
     `api`, `runtime`, `persistence`, `commit`, `branch`, `data/kv`,
     `control`, `diagnostics`, `config`, `test_support`, and `testkit`.
   - Keep non-slice modules absent or empty. Do not create JSON/event/vector/
     graph/retrieval scaffolding unless the crate-shape guard requires empty
     module declarations.

2. **Engine API Surface**
   - Add a minimal executor-facing database handle, open options, branch DTOs, KV DTOs,
     and error type.
   - Required executor-facing operations:
     cache open, durable-local open, close, branch list, branch create from
     source head, KV put, KV get, and KV delete.
   - Keep values byte-oriented for the first slice unless the executor-facing
     compatibility layer requires a product value enum.
   - Reject reserved branch names and system-space access through stable errors.

3. **Persistence Adapter**
   - Define engine-owned `RowAddress`, `RowMutation`, `ReadSelector`,
     `CommitPlan`, and `CommitOutcome`.
   - Define a persistence trait or service boundary used by runtime, branch,
     control, and KV code.
   - Implement the storage-next-backed adapter in `persistence`.
   - Add a fake persistence implementation in `test_support` or `testkit` for
     deterministic error/fault tests.
   - Translate engine rows to storage-next L9 `StorageSpaceId`, `StorageKey`,
     `StorageValue`, `CommitBatch`, branch requests, diagnostics, and open
     options only inside this module.

4. **Physical Row Encoding**
   - Implement storage-space registry constants for the first slice:
     KV source rows `0x20`, branch control rows `0x30`, registry rows `0x32`,
     and dataset rows `0x34`.
   - Implement capability-local KV key encoding that is deterministic,
     prefix-scannable, and independent of storage internals.
   - Implement control-row key prefixes for database identity, registry version,
     branch catalog, branch generation, and branch lifecycle state.
   - Add golden or fixture tests for row key stability before durable tests rely
     on those bytes.

5. **Runtime Open**
   - Implement cache open by constructing storage-next cache mode and then
     running control-plane bootstrap.
   - Implement durable-local open by constructing storage-next durable-local
     standard mode and then running control-plane bootstrap/validation.
   - Ensure durable-local requires the local durable capability and never falls
     back to cache.
   - Return stable open outcomes that distinguish created, reopened, rejected,
     and corrupt/incompatible databases.
   - Close the storage runtime through persistence and preserve durability facts
     in the engine close outcome.

6. **Control-Plane Bootstrap**
   - Derive deterministic identities for the system branch and default branch.
   - Create or validate:
     database identity row, engine layout version row, storage-space registry
     version row, default branch catalog row, and KV capability registry row.
   - Validate that existing rows match the current engine layout. Missing or
     corrupt required rows must fail closed unless an explicit repair rule is
     implemented in this slice.
   - Hide `_system_` branch and `_system_` space from ordinary branch/KV APIs.

7. **Branch MVP**
   - Implement branch-name validation and exact reserved-name checks.
   - Implement branch lookup by name through the control-plane branch catalog.
   - Implement branch listing from the branch catalog, excluding system branch.
   - Implement branch creation from an explicit source branch head:
     allocate a new branch identity/generation, create or fork the storage
     branch through persistence, and activate the branch catalog row.
   - Record enough lifecycle state to handle failure between storage branch
     creation and catalog activation. The first slice may fail closed on
     pending/corrupt branch lifecycle rows, but it must not silently list a
     half-created branch as healthy.
   - Defer merge, diff, restore, revert, cherry-pick, rename, delete, and
     branch-from-time/history.

8. **KV MVP**
   - Implement KV put, get, and delete over an explicit branch and product
     space.
   - Use the branch catalog to resolve branch name to branch identity and
     generation before reads or writes.
   - Submit writes through internal commit plans. Include branch generation
     guards where storage-next L9 supports them.
   - Use latest-visible reads for the first slice. Version, timestamp, history,
     scan, and read-set helper surfaces are deferred unless needed by branch
     create or tests.
   - Ensure branch isolation: a KV write on one branch does not mutate another
     branch's visible value.

9. **Error And Diagnostic Mapping**
   - Map storage errors at the persistence boundary into engine errors with
     stable codes, classes, retryability, and source chains.
   - Do not expose storage enum/type names in executor-facing error codes or messages.
   - Expose minimal diagnostics for open mode, runtime state, branch catalog
     health, control-plane layout health, and persistence health.

10. **Source Guards And Closeout**
    - Add dependency guards proving only `persistence` imports storage-next.
    - Add vocabulary guards proving production code does not use planning
      labels.
    - Add source guards proving branch/KV modules do not construct
      storage keys, storage-space IDs, or storage commit batches directly.
    - Run cache and durable-local end-to-end tests before adding any further
      capability.

## Test Plan

### Unit Tests

1. **API Shape**
   - Cache and durable open options are explicit constructors.
   - Open options do not implement an implicit default mode.
   - Engine DTOs do not expose storage-next types.
   - Reserved branch names and system-space names are rejected.

2. **Row Encoding**
   - KV row address maps to storage-space ID `0x20`.
   - Branch catalog rows map to control storage-space ID `0x30`.
   - Registry rows map to control storage-space ID `0x32`.
   - Dataset identity rows map to control storage-space ID `0x34`.
   - User key bytes round-trip without product-space or storage-space
     ambiguity.
   - Encoded row keys are stable against checked fixtures.

3. **Persistence Adapter With Fakes**
   - Successful commit maps to engine commit outcome.
   - Storage conflict maps to an engine conflict error.
   - Ambiguous durable outcome maps to an engine ambiguous-commit error.
   - Storage unavailable maps to retryable engine storage-unavailable error.
   - Corrupt storage diagnostics map to non-retryable engine corruption error.
   - Branch mechanic success/failure maps without exposing storage branch
     operation names through executor-facing errors.

4. **Control-Plane Bootstrap**
   - New cache database creates required control rows.
   - New durable database creates required control rows.
   - Reopen with matching rows succeeds.
   - Missing required identity or registry row fails closed or is repaired only
     if the repair rule is explicitly implemented.
   - Corrupt registry version fails with stable incompatible-layout error.
   - System branch and system space are hidden from product listings.

5. **Branch MVP**
   - Default branch exists after open.
   - Creating a branch from default records a catalog row.
   - Duplicate branch name is rejected.
   - Invalid and reserved branch names are rejected.
   - Branch listing excludes `_system_`.
   - Branch lookup returns stable identity/generation.
   - Pending or corrupt branch lifecycle rows fail closed on open.

6. **KV MVP**
   - Put then get returns the committed value.
   - Missing key returns none, not an error.
   - Delete hides a previously committed key.
   - Overwrite returns the latest committed value.
   - KV writes require a valid branch.
   - KV writes reject reserved/system product spaces.
   - KV read/write code cannot construct storage-next commit batches directly.

### Integration Tests

1. **Cache End To End**
   - Open cache database.
   - Verify default branch.
   - Put/get/delete KV on default branch.
   - Create a branch from default.
   - Verify inherited or copied source value according to branch-create
     semantics.
   - Write a different value on the new branch.
   - Verify default branch value is unchanged.
   - Close successfully with non-durable close facts.

2. **Durable-Local End To End**
   - Open durable-local database in a temp directory.
   - Put KV on default branch.
   - Create a branch and write branch-local KV.
   - Close.
   - Reopen the same directory.
   - Verify branch catalog and KV values are preserved.
   - Verify open outcome reports reopened rather than created.

3. **Branch Create Failure Windows**
   - Inject failure before storage branch creation.
   - Inject failure after storage branch creation but before catalog
     activation.
   - Inject failure after catalog activation but before close.
   - Reopen either repairs according to the implemented rule or fails closed
     with stable diagnostics. No test may accept a silently half-created branch.

4. **Persistence Boundary**
   - Run source guard over `crates/engine-next/src`.
   - Assert only `src/persistence/**` imports `strata_storage_next` or
     `strata-storage-next`.
   - Assert branch, KV, control, runtime, and API modules use only engine-owned
     persistence abstractions.

5. **Mode Boundary**
   - Cache open works without localfs feature or backend handle.
   - Durable-local open requires localfs/backend support.
   - Durable-local never falls back to cache.
   - Cache and durable-local expose the same branch/KV semantics except for
     durable reopen and close facts.

### Verification Commands

```bash
cargo fmt --all --check
cargo check -p strata-engine-next --all-features
cargo test -p strata-engine-next --test dependency_guards --all-features
cargo test -p strata-engine-next --test persistence_adapter --all-features
cargo test -p strata-engine-next --test control_plane --all-features
cargo test -p strata-engine-next --test branch_and_kv --all-features
cargo test -p strata-engine-next --all-features
```

### Branch And KV Exit Gates

1. `crates/engine-next` builds without unsafe code.
2. Cache open creates a usable database with default branch and KV support.
3. Durable-local open creates and reopens a usable database with preserved
   branch catalog and KV rows.
4. Branch create from an explicit source branch head works through engine
   branch APIs.
5. KV put/get/delete works on default and created branches.
6. Branch isolation is proven by tests.
7. Storage-next imports are isolated to `persistence`.
8. Engine errors and DTOs do not expose storage-next type names.
9. The first slice contains no JSON, event, vector, graph, retrieval, IPC,
   clone, merge, diff, restore, or public transaction-session implementation.

## Non-Goals

1. No product data capability implementation beyond the branch and KV vertical
   slice described above.
2. No IPC service implementation.
3. No intelligence or inference integration.
4. No public compatibility layer for old engine internals.
5. No JSON, event, vector, graph, retrieval, clone, merge, diff, restore,
   revert, cherry-pick, or branch-from-history implementation.
6. No public manual transaction sessions.
7. No direct storage-next access outside the persistence adapter.

## Milestone Exit Gate

M5 is complete when engine-next can open cache and durable databases, bootstrap
control-plane rows, create product branches, and read/write KV rows through the
persistence adapter only. The roadmap Test Gate Summary remains the canonical
milestone gate; this plan explains how M5 reaches it.
