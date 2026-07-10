# M5G Implementation Plan: Engine-Next Branch And KV Vertical Spine

Status: draft implementation plan

Planning labels such as `M5G` are documentation metadata only. They must not
appear in production module names, type names, function names, feature flags,
error codes, metrics, tests, or production comments.

## Goal

Create the first usable engine-next vertical slice:

1. Open a new cache database.
2. Open and reopen a durable-local database.
3. Bootstrap minimal engine control-plane rows.
4. Create product branches.
5. Put, get, and delete KV records on default and created branches.
6. Prove that all storage-next access is isolated behind the engine persistence
   adapter.

This is not a general product rewrite. The slice deliberately implements only
the branch and KV behavior needed to prove that engine-next can consume
storage-next L9 as its persistence boundary.

## Parent Plans And Contracts

1. `docs/architecture/implementation-plans/m5-m5t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m5g-branch-kv-vertical-spine-test-plan.md`
3. `docs/architecture/engine-architecture.md`
4. `docs/architecture/engine/target-crate-shape-and-test-harness.md`
5. `docs/architecture/engine/persistence-adapter-contract.md`
6. `docs/architecture/engine/control-plane-layout-contract.md`
7. `docs/architecture/engine/storage-space-id-registry.md`
8. `docs/architecture/storage/l9-storage-api-boundary.md`

## Scope

In scope:

1. `crates/engine-next` crate skeleton.
2. Executor-facing engine-next open, branch, and KV API surface.
3. Engine-owned persistence adapter over storage-next L9.
4. Engine-owned row address, row mutation, read selector, commit plan, and
   commit outcome vocabulary.
5. Physical row encoding for KV and the minimal control rows needed by this
   slice.
6. Cache and durable-local runtime open/close.
7. Minimal control-plane bootstrap and validation.
8. Product branch create/list/lookup from the engine branch catalog.
9. KV put/get/delete on explicit branch and product space.
10. Error mapping and minimal diagnostics.
11. Dependency/source guards.

Out of scope:

1. JSON, event, vector, graph, retrieval, orchestration, IPC, clone, and
   intelligence integration.
2. Branch merge, diff, restore, revert, cherry-pick, rename, delete, branch from
   timestamp/version/history, branch tags, notes, and bundles.
3. Public manual transaction sessions.
4. Compatibility wrapper for the old engine API.
5. Direct storage-next calls outside `persistence`.

## Binding Decisions

1. **Branch and KV are the first vertical proof.** KV is the smallest product
   capability over the branch-aware MVCC row substrate, and branch creation is
   the smallest useful product control-plane workflow.
2. **Persistence is the only storage-facing module.** `api`, `runtime`,
   `branch`, `data`, `control`, `diagnostics`, and `config` must not import
   storage-next or construct storage-next API objects.
3. **Engine owns product branch names.** Storage-next receives opaque branch
   identities and generation facts. Engine-next owns names, reserved-name
   validation, catalog rows, and product lifecycle state.
4. **Branch create has an explicit failure window.** The storage branch
   operation and the engine branch catalog activation are separate facts. The
   implementation must either repair/complete/tombstone pending rows on open or
   fail closed with stable diagnostics. It must never silently expose a
   half-created branch as healthy.
5. **Every executor-facing write is one internal commit plan.** KV put/delete
   and branch catalog mutations build engine commit plans and submit them
   through persistence. Public begin/commit/rollback sessions are not
   introduced.
6. **Cache and durable-local share semantics.** Cache is explicit and volatile.
   Durable-local adds persistence and close/reopen proof. Durable-local must
   never silently fall back to cache.
7. **Control rows are engine product rows.** Database identity, layout version,
   storage-space registry version, branch catalog, and capability registry rows
   are written through the same persistence adapter as ordinary rows.
8. **Storage-space IDs follow the registry.** KV source rows use `0x20`.
   Branch control rows use `0x30`. Registry rows use `0x32`. Dataset/database
   identity rows use `0x34`.
9. **The system branch and system space are hidden axes.** `_system_` branch and
   `_system_` space are used internally and must not be returned through normal
   branch or space listing.
10. **The first slice is byte-oriented.** KV values and keys may be
    executor-facing bytes in this slice. Product value enums can be added later
    by an executor/API compatibility or capability plan if needed.

## Target Crate Shape

Create only the modules needed for the vertical slice:

```text
crates/engine-next/
  Cargo.toml
  src/
    lib.rs
    api/
    runtime/
    persistence/
    commit/
    branch/
    data/
      mod.rs
      kv/
    control/
    diagnostics/
    config/
    test_support/
    testkit/
  tests/
    common/
    dependency_guards.rs
    persistence_adapter.rs
    control_plane.rs
    branch_and_kv.rs
```

Do not scaffold `json`, `event`, `vector`, `graph`, `retrieval`, `ipc`,
`clone`, or `orchestration` modules in this slice unless the crate-shape guard
requires an empty placeholder. If placeholders are required, they must not have
behavior.

## Executor-Facing API Shape

The exact Rust signatures are implementation details, but the first slice should
provide this engine contract for executor-next:

1. `Database::open_cache(options)` or equivalent explicit cache constructor.
2. `Database::open_local(path, options)` or equivalent explicit durable-local
   constructor.
3. `Database::close()`.
4. `Database::branches().list()`.
5. `Database::branches().create_from_head(source, name)`.
6. `Database::kv(branch, space).put(key, value)`.
7. `Database::kv(branch, space).get(key)`.
8. `Database::kv(branch, space).delete(key)`.

Engine DTOs should include:

1. Database open options and open outcome.
2. Branch name, branch summary, and branch create outcome.
3. Space name or product-space selector.
4. KV key and KV value bytes.
5. Commit outcome summary.
6. Close outcome summary.
7. Engine error with stable code, class, retryability, and source chain.

Engine DTOs must not expose storage-next types, storage-space IDs, WAL facts,
manifest facts, table objects, storage branch operation enums, or lower-layer
storage errors.

## Engine Internal Vocabulary

These are engine-owned concepts. They may live in `persistence`, `commit`, or
small adjacent modules depending on the final code shape:

1. `RowAddress`: branch identity, product space, symbolic row class, and
   capability-local key bytes.
2. `RowMutation`: put/delete against a row address, optional TTL, and operation
   origin.
3. `ReadSelector`: latest-visible selector for this slice, with room for future
   version/timestamp/history selectors.
4. `CommitPlan`: single-branch internal write unit containing row mutations,
   guards, origin, and durability expectation.
5. `CommitOutcome`: committed version, timestamp, mutation counts, durability
   summary, and ambiguity/backpressure facts.
6. `BranchCatalogRecord`: product branch name, branch identity, generation,
   source branch, lifecycle state, and created metadata.
7. `DatabaseIdentityRecord`: dataset/database identity and layout version.
8. `RegistryRecord`: storage-space registry version and supported capability
   assignments.

## Control-Plane Minimum

The first slice needs only these row families:

| Row family | Branch | Product space | Storage-space ID | Purpose |
|---|---|---|---|---|
| Database identity | `_system_` | `_system_` | `0x34` | Stable database identity and engine layout version. |
| Storage-space registry | `_system_` | `_system_` | `0x32` | Engine row assignment version and supported IDs. |
| Capability registry | `_system_` | `_system_` | `0x32` | KV capability support marker. |
| Branch catalog | `_system_` | `_system_` | `0x30` | Product branch name to branch identity/generation. |
| Branch lifecycle | `_system_` | `_system_` | `0x30` | Pending/active/corrupt branch create state. |

The default branch must be created and recorded during bootstrap. The system
branch identity may be deterministic because the branch catalog itself lives on
the system branch.

## Implementation Order

### 1. Crate Skeleton

Files:

1. `crates/engine-next/Cargo.toml`
2. `crates/engine-next/src/lib.rs`
3. Module `mod.rs` files for the target slice shape.
4. Workspace `Cargo.toml` membership.

Tasks:

1. Add `strata-engine-next` package.
2. Inherit workspace lints.
3. Add `#![deny(unsafe_code)]`.
4. Add features needed for this slice, such as `default`, `localfs`, and
   `testkit`, if the existing workspace pattern requires them.
5. Depend on `strata-core-next` and `strata-storage-next` only where needed.
6. Re-export only the executor-facing API module from `lib.rs`.
7. Add empty tests that prove the crate builds before behavior lands.

Exit gate:

1. `cargo check -p strata-engine-next --all-features` succeeds.
2. No production module contains planning labels.

### 2. API And Error Surface

Files:

1. `src/api/mod.rs`
2. `src/api/database.rs`
3. `src/api/branch.rs`
4. `src/api/kv.rs`
5. `src/api/options.rs`
6. `src/diagnostics/error.rs`

Tasks:

1. Define executor-facing database handle and explicit open constructors.
2. Define branch name validation rules with exact `_system_` rejection.
3. Define product-space validation with exact `_system_` rejection for normal
   user KV access.
4. Define byte-oriented KV key/value wrappers or use small validated newtypes.
5. Define engine error type with code, class, retryability, message, and source
   chain.
6. Keep the executor-facing surface synchronous.
7. Avoid a `Default` that opens cache implicitly.

Exit gate:

1. API unit tests prove explicit cache/durable constructors and validation.
2. Engine API signatures do not expose storage-next types.

### 3. Persistence Contract

Files:

1. `src/persistence/mod.rs`
2. `src/persistence/adapter.rs`
3. `src/persistence/plan.rs`
4. `src/persistence/row.rs`
5. `src/persistence/error.rs`
6. `src/test_support/persistence.rs`

Tasks:

1. Define the internal persistence trait/service used by runtime, control,
   branch, and KV modules.
2. Define row address, mutation, selector, commit plan, and outcome structures.
3. Add fake persistence implementation for deterministic tests.
4. Add storage-next-backed persistence implementation.
5. Translate storage-next open, branch, commit, read, diagnostics, maintenance,
   and close APIs only inside `persistence`.
6. Map storage-next error classes to engine error classes.
7. Preserve storage source chains internally without exposing storage enum
   names in executor-facing error strings/codes.

Exit gate:

1. Persistence fake tests cover success, conflict, unavailable storage,
   corruption, and ambiguous commit.
2. Dependency guard proves storage-next imports are isolated to `persistence`.

### 4. Row Encoding And Registry

Files:

1. `src/persistence/space.rs`
2. `src/persistence/key.rs`
3. `src/control/records.rs`
4. `src/data/kv/codec.rs`

Tasks:

1. Add symbolic row class constants for KV, branch control, registry, and
   dataset identity.
2. Resolve symbolic row classes to registry byte assignments inside
   persistence.
3. Encode KV row keys deterministically from product key bytes.
4. Encode control row keys with stable prefixes and versioned payloads.
5. Add decode/validate routines for control rows.
6. Add checked fixtures for encoded key bytes and control payload round trips.

Exit gate:

1. Row encoding fixture tests pass.
2. Capability/control code never sees raw storage-next `StorageSpaceId`.

### 5. Runtime Open And Close

Files:

1. `src/runtime/mod.rs`
2. `src/runtime/open.rs`
3. `src/runtime/close.rs`
4. `src/config/mod.rs`

Tasks:

1. Implement cache open through persistence using storage-next cache mode.
2. Implement durable-local open through persistence using storage-next
   durable-local standard mode.
3. Reject durable-local open if required backend/localfs capability is not
   available.
4. Never fall back from durable-local to cache.
5. Run control-plane bootstrap after storage open and before returning the
   executor-facing database handle.
6. Preserve open outcome facts: created, reopened, rejected, incompatible, or
   corrupt.
7. Implement close through persistence and preserve durable/non-durable close
   facts.

Exit gate:

1. Cache open/close tests pass.
2. Durable-local create/reopen/close tests pass.

### 6. Control-Plane Bootstrap

Files:

1. `src/control/mod.rs`
2. `src/control/bootstrap.rs`
3. `src/control/catalog.rs`
4. `src/control/registry.rs`
5. `src/control/identity.rs`

Tasks:

1. Derive deterministic system branch identity.
2. Derive or allocate default branch identity.
3. Create required control rows on new database open.
4. Validate required control rows on reopen.
5. Validate storage-space registry version and KV capability support.
6. Create default branch catalog row if absent in a new database.
7. Reject missing/corrupt required rows on an existing database unless an
   explicit repair path exists in this slice.
8. Hide system branch and system space from product listings.

Exit gate:

1. New cache and durable opens create the required rows.
2. Reopen validates matching rows.
3. Corrupt or incompatible rows fail closed with stable engine errors.

### 7. Branch MVP

Files:

1. `src/branch/mod.rs`
2. `src/branch/name.rs`
3. `src/branch/catalog.rs`
4. `src/branch/service.rs`

Tasks:

1. Validate product branch names.
2. Lookup branch summaries by name through the engine branch catalog.
3. List active product branches while hiding the system branch.
4. Create a branch from an explicit source branch head.
5. Allocate or derive branch identity and generation.
6. Call storage-next branch mechanics through persistence.
7. Write pending lifecycle row before or during create as required by the chosen
   failure-window strategy.
8. Activate branch catalog row only after storage branch mechanics succeed.
9. On open, fail closed or repair/tombstone pending lifecycle rows according to
   the implemented strategy.

Exit gate:

1. Default branch exists after open.
2. Branch create/list/lookup tests pass.
3. Failure windows do not silently expose half-created branches.

### 8. KV MVP

Files:

1. `src/data/mod.rs`
2. `src/data/kv/mod.rs`
3. `src/data/kv/service.rs`
4. `src/data/kv/types.rs`

Tasks:

1. Resolve branch name to branch identity and generation before reads/writes.
2. Validate product space and key.
3. Build row addresses using symbolic KV source row class.
4. Implement put as one internal commit plan.
5. Implement delete as one internal commit plan.
6. Implement get as latest-visible persistence read.
7. Include branch generation guard in commit plans where storage-next L9
   supports it.
8. Return committed value bytes and commit facts through engine DTOs.
9. Preserve branch isolation.

Exit gate:

1. Put/get/delete/overwrite tests pass on default branch.
2. Branch isolation tests pass on a created branch.
3. Durable reopen preserves KV rows and branch catalog rows.

### 9. Error Mapping And Diagnostics

Files:

1. `src/diagnostics/mod.rs`
2. `src/diagnostics/error.rs`
3. `src/diagnostics/health.rs`

Tasks:

1. Define stable error classes and codes for this slice:
   invalid input, not found, conflict, unavailable, ambiguous commit,
   incompatible layout, corruption, closed runtime, and internal bug.
2. Map persistence/storage failures to engine errors only at the boundary.
3. Add diagnostics for open mode, runtime state, control-plane health, branch
   catalog health, and persistence health.
4. Keep storage enum names and lower-layer type names out of engine errors.
5. Preserve source chain for debugging.

Exit gate:

1. Error mapping tests pass against fake persistence failures.
2. Engine errors are stable and do not expose storage-next type names.

### 10. Source Guards And Closeout

Files:

1. `tests/dependency_guards.rs`
2. `tests/common/source_guard.rs`
3. `tests/branch_and_kv.rs`
4. `tests/control_plane.rs`
5. `tests/persistence_adapter.rs`

Tasks:

1. Add dependency guard for storage-next imports.
2. Add production vocabulary guard against planning labels.
3. Add source guard that API, branch, KV, control, and runtime modules do not
   construct storage-next commit batches, storage keys, or storage-space IDs.
4. Add source guard that engine APIs do not expose storage-next types.
5. Run all branch/KV conformance tests.

Exit gate:

1. All verification commands pass.
2. No out-of-scope capability code has behavior.

## Stop Conditions

1. If storage-next L9 cannot express branch creation plus catalog write
   correctness without a failure window that can be detected or repaired, stop
   and write the missing storage/engine boundary plan.
2. If KV requires read-set validation for correctness in the first slice, stop
   and finish the L9 read-set fact surface before continuing.
3. If durable-local open requires cache fallback to pass tests, stop and fix the
   durable open path rather than weakening the mode contract.
4. If a module outside `persistence` needs to import storage-next to make the
   slice work, stop and widen the persistence adapter instead.
5. If branch create semantics drift into merge, diff, restore, or branch
   history, stop and split that work into a later branch plan.
6. If control-plane bootstrap requires more row families than listed here, add a
   control-plane subplan before implementing them.

## Verification Commands

```bash
cargo fmt --all --check
cargo check -p strata-engine-next --all-features
cargo test -p strata-engine-next --test dependency_guards --all-features
cargo test -p strata-engine-next --test persistence_adapter --all-features
cargo test -p strata-engine-next --test control_plane --all-features
cargo test -p strata-engine-next --test branch_and_kv --all-features
cargo test -p strata-engine-next --all-features
```

## Exit Gate

The slice is complete when:

1. `crates/engine-next` builds with unsafe denied.
2. Cache open creates a usable database with default branch and KV support.
3. Durable-local open creates and reopens a usable database with preserved
   branch catalog and KV rows.
4. Branch create from an explicit source branch head works through engine branch
   APIs.
5. KV put/get/delete works on default and created branches.
6. Branch isolation is proven.
7. Branch create failure windows are detected, repaired, or fail closed.
8. Storage-next imports are isolated to `persistence`.
9. Engine errors and DTOs do not expose storage-next type names.
10. No JSON, event, vector, graph, retrieval, IPC, clone, merge, diff, restore,
    or public transaction-session behavior lands in this slice.
