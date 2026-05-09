# EG7 Implementation Plan

## Purpose

`EG7` removes executor's remaining storage bypasses after engine has absorbed
security, product open behavior, graph, vector, and search.

The target stack is:

```text
core -> storage -> engine -> intelligence -> executor -> cli
```

In that stack, executor is a command/session/product handle layer. It may own
public command DTOs, IPC adaptation, session state, and user-facing error
shape, but it must not construct storage keys, storage namespaces, storage type
tags, or interpret storage-local errors. Those details are engine-owned once
the primitive runtimes live in engine.

This is the single implementation plan for the `EG7` phase. Lettered sections
such as `EG7A`, `EG7B`, and `EG7C` are tracked in this file rather than in
separate letter-specific documents.

Read this with:

- [engine-consolidation-plan.md](./engine-consolidation-plan.md)
- [engine-crate-map.md](./engine-crate-map.md)
- [eg1-implementation-plan.md](./eg1-implementation-plan.md)
- [eg2-implementation-plan.md](./eg2-implementation-plan.md)
- [eg3-implementation-plan.md](./eg3-implementation-plan.md)
- [eg4-implementation-plan.md](./eg4-implementation-plan.md)
- [eg5-implementation-plan.md](./eg5-implementation-plan.md)
- [eg6-implementation-plan.md](./eg6-implementation-plan.md)
- [../storage/v1-storage-consumption-contract.md](../../storage/v1-storage-consumption-contract.md)

## Scope

`EG7` owns:

- removing the normal `strata-executor -> strata-storage` dependency
- removing executor imports of `strata_storage::*`
- removing executor public re-exports of storage `Key` and `Namespace`
- replacing executor's direct use of `validate_space_name` with an
  engine-owned validation API
- removing executor's direct `StorageError -> Error` conversion path
- moving space deletion orchestration into engine
- removing storage key, namespace, and type-tag construction from executor
  transactional KV/event/space paths
- replacing executor production use of raw storage transaction context APIs
  with engine-owned transaction/session APIs
- preserving command behavior, IPC behavior, session defaults, and public
  output shape unless a behavior fix is explicitly called out
- tightening guard tests so executor cannot regain direct storage access

`EG7` does not own:

- collapsing executor into engine
- redesigning the command enum or public executor output DTOs
- changing storage key encoding, WAL, manifest, checkpoint, or snapshot format
- changing primitive semantics
- redesigning user-facing error enums beyond the minimum conversion cleanup
- removing root dev/test dependencies on storage where tests intentionally
  inspect storage behavior
- changing intelligence, CLI, or product API behavior beyond dependency guard
  updates

## Load-Bearing Boundary Rules

Do not solve `EG7` by moving storage types through engine re-exports.

Engine may keep transitional storage re-exports for engine-local modules and
tests, but executor production code should not import or rely on:

- `strata_storage::Key`
- `strata_storage::Namespace`
- `strata_storage::TypeTag`
- `strata_storage::StorageError`
- `strata_storage::validate_space_name`
- `strata_engine::TransactionContext` when the call is really a storage
  transaction-context operation

Executor may keep:

- `strata_engine::Database`
- `strata_engine::Transaction`, if the methods used are engine-owned and do
  not expose storage `Key`, `Namespace`, or raw transaction context mutation
- engine primitive facades such as `KVStore`, `JsonStore`, `EventLog`,
  `PrimitiveGraphStore`, `VectorStore`, `SpaceIndex`, and search types while
  executor still dispatches commands through those facades
- public command/session state such as current branch, current space, access
  mode, and IPC state

The practical rule is:

```text
executor says what product operation to run
engine decides which storage rows, runtime hooks, and side effects implement it
storage executes generic mechanics
```

## Starting State

At the start of `EG7`, executor is the remaining production crate above engine
with a normal storage dependency.

The verified direct executor storage touchpoints are:

| File | Storage use | Required move |
| --- | --- | --- |
| `crates/executor/Cargo.toml` | normal `strata-storage` dependency | remove after production imports are gone |
| `crates/executor/src/lib.rs` | `pub use strata_storage::{Key, Namespace}` | delete or replace with engine-owned public types only if still needed |
| `crates/executor/src/compat.rs` | `validate_space_name` | call engine-owned space validation |
| `crates/executor/src/bridge.rs` | `validate_space_name` | call engine-owned space validation |
| `crates/executor/src/convert.rs` | `StorageError` conversion | remove after storage errors are mapped inside engine |
| `crates/executor/src/session.rs` | `Namespace` construction for manual transactions | use engine transaction scope APIs |
| `crates/executor/src/handlers/kv.rs` | `Key`, `Namespace`, `TypeTag` in transaction commands | use engine transactional KV APIs |
| `crates/executor/src/handlers/json.rs` | `Namespace` in transaction commands | use engine transaction scope APIs |
| `crates/executor/src/handlers/event.rs` | `Key`, `Namespace` in transaction commands | add engine transactional event query APIs |
| `crates/executor/src/handlers/space_delete.rs` | `Key`, `Namespace`, `TypeTag` and raw scans/deletes | move complete space delete orchestration into engine |
| `crates/executor/src/session.rs` tests | `Key`, `Namespace` corruption fixture | replace with engine-owned test helper or move fixture into engine tests |

That is ten production files or manifests, plus one test-only fixture row in
`session.rs`.

At the start of `EG7`, `strata-intelligence`, `strata-cli`, and root `src`
production code do not import `strata_storage::*`. The root package still has
storage dev/test uses, which are outside this phase unless they become
production dependencies above engine.

At the start of `EG7`, `cargo tree -p strata-executor --edges normal` still
contains `strata-storage` both directly and through engine. The direct edge is
the one `EG7` must remove; the transitive edge through engine is expected.

## Target Engine Surfaces

`EG7` should add narrow engine APIs instead of broad storage facades.

### Space Validation

Engine should expose one public validation entry point for executor:

```text
validate_space_name(space: &str) -> StrataResult<()>
```

or an equivalent `SpaceName`/`SpaceRef` newtype if the existing engine style
points that way.

The engine implementation may delegate to storage's physical layout validator,
but it must return `StrataError`, not a storage-local string or
`StorageError`. Executor then maps `StrataError` through its existing
`Error::from(StrataError)` path.

### Transaction Scope

Manual executor sessions currently create storage namespaces and sometimes
pull out raw `TransactionContext`. Engine should provide a transaction scope
surface that lets executor say:

```text
transaction on branch X, scoped to space Y, execute primitive operation Z
```

without constructing storage keys or namespaces.

Likely shape:

- `Transaction::scope_space(space: &str) -> ScopedTransaction<'_>` or
  equivalent
- `ScopedTransaction` or a replacement engine wrapper supports the KV, JSON,
  event, graph, and vector operations executor can run inside a transaction
- methods return `StrataResult<T>` and engine-owned DTOs
- no executor handler receives `Namespace`, `Key`, `TypeTag`, or raw storage
  transaction context

The existing `TransactionOps` trait covers much of KV/JSON/event, but it does
not cover every executor transaction command. `EG7` should close only the
needed gaps, such as:

- transaction-aware KV scan with start/limit behavior
- transaction-aware KV batch read/delete/exists helpers if a helper preserves
  current output/error behavior better than repeating loops in executor
- transaction-aware event type-index lookup
- transaction-aware event exists and batch append behavior with read-your-writes
- graph/vector transaction entry points currently reached through raw
  `TransactionContext`

### Space Deletion

Space deletion should become one engine-owned operation because it coordinates:

- branch existence
- default and system-space protection
- optional emptiness check
- KV, JSON, event, graph, and vector data deletion
- vector collection purge
- search document removal
- product-layer shadow embedding cleanup through a hook owned above engine
- space metadata deletion
- logging and partial-failure policy

Executor should call one semantic engine API and translate the final
`StrataResult` into `Output::Unit`.

Likely shape:

```text
delete_space(branch_id, space, options) -> StrataResult<SpaceDeleteOutcome>
```

where the outcome can initially be minimal. If engine already has a better
space service home, use that instead of inventing a parallel facade.

### Error Boundary

Storage-origin errors should cross into executor only as `StrataError` or a
more specific engine error. Executor should not contain an
`impl From<StorageError> for Error` after `EG7`.

The executor conversion layer should remain responsible for user-facing
executor error shape, but it should not know storage variants.

## Implementation Sections

### EG7A - Rebaseline And Characterize

Status: complete.

Re-run the executor storage import inventory and convert it into a code-level
checklist.

Tasks:

- record every production `strata_storage::*` import in executor
- record every executor production use of `strata_engine::TransactionContext`
  or `Transaction::context_mut()` that expresses product behavior rather than
  engine internals
- identify tests that intentionally inspect raw storage and decide whether they
  should move to engine tests or use engine-owned helpers
- add or identify characterization coverage before behavior moves
- update this document if the inventory finds another live storage touchpoint

Characterization should cover at least:

- `Strata::set_space` and `Session::set_space` validation errors
- in-transaction KV get/list/scan/put/delete/batch behavior
- in-transaction JSON get/set/delete/list/batch behavior
- in-transaction event append/get/exists/get-by-type/len/batch behavior
- in-transaction graph and vector commands that currently use raw transaction
  context
- transaction side effects after commit for search and embeddings
- forced and non-forced space deletion
- space deletion of default and system spaces
- space deletion side effects across KV, JSON, event, graph, vector, search,
  and the product-layer intelligence shadow cleanup hook
- IPC-backed handle behavior where the client still sends executor commands

Exit criteria:

- the executor storage bypass list is current and committed to this plan
- missing engine APIs are named before implementation starts
- behavior tests exist or are explicitly named as existing coverage

Implementation result:

- Direct storage imports are the ten production files or manifests listed in
  [Starting State](#starting-state), plus the `session.rs` test-only corruption
  fixture listed there.
- Raw storage transaction-context use is concentrated in:
  - `crates/executor/src/session.rs` - creates storage `Namespace` values and
    calls `Transaction::context_mut()` while dispatching product commands
  - `crates/executor/src/handlers/kv.rs` - uses raw `TransactionContext`,
    `Key`, and `TypeTag` for transactional KV reads, lists, scans, deletes,
    batches, and existence checks
  - `crates/executor/src/handlers/event.rs` - uses raw `TransactionContext`
    and event storage keys for transactional type-index lookup
  - `crates/executor/src/handlers/json.rs` - receives storage `Namespace` for
    scoped JSON transaction operations
  - `crates/executor/src/handlers/graph.rs` and
    `crates/executor/src/handlers/vector.rs` - receive raw
    `TransactionContext` for engine extension methods during transaction
    dispatch
- Added `tests/executor/eg7_characterization.rs` with focused behavior
  coverage for:
  - typed-handle and session space validation error variants and reason text
  - transaction commands with omitted space using the active session space for
    KV, JSON, and event operations, including transactional exists, scan, and
    non-empty event batch append paths
  - forced space deletion clearing mixed target-space data while preserving a
    sibling space, including sibling graph and vector data
- Existing characterization coverage that `EG7B`-`EG7D` should continue to
  rely on:
  - `tests/executor/session_transactions.rs` covers transaction lifecycle,
    KV/JSON/event read-your-writes behavior, as-of bypass behavior, graph and
    vector transaction behavior, batch behavior, validation errors, commit
    visibility, and rollback discard semantics
  - `tests/executor/command_dispatch.rs` covers space create/list/exists,
    forced and non-forced space deletion, default-space protection, graph and
    vector data cleanup, search hit cleanup, shadow embedding cleanup, and
    describe-space counts
  - `tests/executor_ex6_runtime.rs` covers product-handle context behavior,
    IPC transaction round trips, read-only write rejection, and product runtime
    subsystem order

Missing engine APIs confirmed by the rebaseline:

- engine-owned space validation returning `StrataResult<()>`
- engine-owned transaction space scope that constructs storage namespaces
  inside engine
- scoped transaction methods for the KV/event cases that currently need raw
  storage keys, especially KV scan/list/delete/batch helpers and event
  get-by-type
- graph/vector transaction dispatch surfaces that do not require executor to
  touch raw `TransactionContext`
- engine-owned forced space deletion operation with current best-effort
  side-effect policy preserved

### EG7B - Engine Space And Error Surface

Status: complete.

Add the small engine-owned surface needed to remove executor's easiest storage
imports first.

Tasks:

- add an engine-owned space-name validation API
- route `compat.rs`, `bridge.rs`, and space command handlers through that API
- ensure validation failures become `StrataError::InvalidInput` before
  executor converts them
- remove executor's direct `StorageError` conversion once no production call
  path needs it
- add tests that assert executor-visible error shape is unchanged for invalid
  spaces and representative storage-backed failures

Exit criteria:

- executor no longer imports `validate_space_name` from storage
- executor no longer imports `StorageError`
- no user-facing error messages drift unless explicitly approved

Implementation result:

- `strata_engine::validate_space_name(space: &str) -> StrataResult<()>`
  is exported from the engine-owned space primitive and maps the existing
  storage namespace rule failure strings into `StrataError::InvalidInput`
- `compat.rs`, `bridge.rs`, `SpaceCreate`, `SpaceExists`, and `SpaceDelete`
  now call the engine validation surface; `SpaceDelete` still preserves the
  existing `default`/`_system_` constraint error before validating ordinary
  user space names
- executor's direct `impl From<StorageError> for Error` has been removed
- before `EG7C`, the remaining raw transaction KV/event storage calls mapped
  storage-local failures through
  `strata_engine::storage_error_for_product_boundary`, a narrow engine-owned
  adapter that preserved the historical executor-facing categories for storage
  invalid-input, capacity, corruption, and I/O failures
- EG7 characterization keeps the invalid-space user-facing message locked
  across typed handle, session, `SpaceExists`, and `SpaceDelete` paths, and
  engine bridge tests lock the representative storage-backed product-boundary
  conversions while executor conversion tests lock their user-facing shapes

### EG7C - Engine Transaction Session Surface

Status: complete.

Replace executor's storage-shaped manual transaction plumbing with engine-owned
transaction APIs.

Tasks:

- add a transaction scope helper that constructs the storage namespace inside
  engine
- add the missing scoped KV/event helpers needed by executor transaction
  commands
- move graph/vector transaction entry points off raw `TransactionContext` where
  executor currently reaches engine extension methods through the storage
  context
- update `session.rs` so it never constructs `Namespace` and does not call
  `Transaction::context_mut()` for product command dispatch
- update KV, JSON, event, graph, and vector transaction handlers to receive an
  engine transaction scope or command facade rather than storage context
- preserve `TxnSideEffects` behavior, or move it into engine if that makes the
  post-commit search/embedding side effects easier to keep consistent

Exit criteria:

- executor production code has no `Namespace`, `Key`, `TypeTag`, or
  `TransactionContext` import
- in-transaction command behavior remains unchanged
- post-commit search and embedding side effects still run once per affected
  entity
- transaction conflict and abort error mapping remains executor-compatible

Implementation result:

- `strata_engine::Transaction::scoped_space(space)` now constructs the
  branch/space storage namespace inside engine and returns the existing scoped
  transaction primitive wrapper without exposing `Namespace` above engine.
- Scoped engine transactions now expose the executor-needed storage-shaped
  read helpers as engine methods:
  - `kv_get_value`
  - `kv_scan`
  - `event_get_by_type`
- Owned engine transactions now expose graph and vector transaction entry
  points so executor no longer reaches those runtimes through raw
  `TransactionContext`. Those entry points derive the branch from the active
  transaction instead of accepting a caller-supplied `BranchId`.
- `session.rs` no longer constructs transaction namespaces and no longer calls
  `Transaction::context_mut()` for product command dispatch. It also uses an
  engine-owned transaction ID accessor for `TxnInfo` instead of reading the raw
  storage transaction context through `Deref`. It passes the owned engine
  transaction directly to the primitive command handlers.
- Owned engine transactions no longer expose the raw storage context through
  production `Deref`/`DerefMut`, public `context_mut`, or public
  `scoped(Arc<Namespace>)`; the raw deref compatibility remains available only
  under `cfg(test)` for existing engine unit fixtures.
- KV, JSON, event, graph, and vector transaction handlers now receive
  `strata_engine::Transaction` instead of storage `TransactionContext`,
  `Namespace`, `Key`, or `TypeTag`.
- The storage bypass guard allowlist was tightened by removing the completed
  `kv.rs`, `json.rs`, `event.rs`, and `session.rs` entries. The guard now scans
  Rust code while ignoring inline `#[cfg(test)]` modules, so a test-only
  corruption fixture does not allow production storage references in the same
  file.
- The remaining executor storage references are the planned `EG7D` space
  deletion path, the planned `EG7E` public re-export/dependency closeout, and
  the `session.rs` test-only corruption fixture.

### EG7D - Engine Space Deletion Ownership

Status: complete.

Move `handlers/space_delete.rs` orchestration into engine.

Tasks:

- add an engine-owned space deletion operation with options for `force`
- preserve default-space and system-space protection
- preserve non-forced emptiness checks
- delete all primitive data families through engine-owned storage access
- purge vector collection state and sidecar files through engine-owned vector
  APIs
- remove search documents for the deleted space through engine-owned search
  APIs
- expose a fallible post-data/pre-metadata cleanup hook so executor can invoke
  intelligence-owned persisted and pending shadow embedding cleanup without
  making engine depend on intelligence internals
- delete space metadata after data cleanup
- characterize partial-failure behavior and preserve current warn-vs-fail
  policy unless a behavior fix is explicitly called out
- cut executor `SpaceDelete` handling to the engine operation

Exit criteria:

- executor `space_delete` handler does not import storage or loop over type
  tags
- forced and non-forced deletion behavior is preserved
- all primitive side effects are tested through executor-visible commands and,
  where needed, engine-local assertions

Implementation result:

- `SpaceIndex::delete_user_space(branch_name, space, force)` is now the
  engine-owned operation for deleting a space's engine-owned data. It preserves
  the existing order: protect `default`/`_system_`, validate the space name,
  verify branch existence, enforce the non-forced emptiness check, delete
  primitive rows, clean engine runtime side effects, then delete space metadata.
- Engine now owns the storage-shaped cleanup previously in
  `handlers/space_delete.rs`: vector rows are deleted through the vector
  runtime, KV/event/JSON/graph rows are scanned by type tag inside engine, and
  executor no longer constructs `Key`, `Namespace`, or `TypeTag` for space
  deletion.
- Search document cleanup and vector collection purge moved behind the engine
  operation with the previous warn-and-continue policy for best-effort runtime
  cleanup failures. `SpaceIndex::delete_user_space_with_post_data_cleanup`
  gives executor a fallible generic post-data/pre-metadata hook so
  intelligence-owned persisted shadow embeddings and the feature-gated
  auto-embed pending queue can be cleaned at the same point as before without
  reintroducing executor storage access or an engine dependency on
  intelligence shadow-key internals.
- Executor `SpaceDelete` now calls the engine operation and translates only the
  two preserved space-delete constraint reasons into executor
  `ConstraintViolation`; all other failures cross as normal `StrataError`
  conversions.
- The storage bypass guard allowlist no longer contains
  `crates/executor/src/handlers/space_delete.rs`.
- Engine-local tests cover protected/non-forced rejection and forced cleanup
  across primitive rows, search index entries, vector collection state,
  non-default branch isolation, and sibling-space preservation. Existing executor
  characterization continues to cover command-visible forced/non-forced delete
  behavior, default-space protection, search cleanup, vector cleanup, graph
  cleanup, and shadow cleanup.

### EG7E - Dependency Removal And Guard Closeout

Status: complete.

Delete the executor storage surface after the call sites are gone.

Tasks:

- remove `strata-storage` from `crates/executor/Cargo.toml`
- remove `pub use strata_storage::{Key, Namespace}` from
  `crates/executor/src/lib.rs`
- update in-repo consumers and tests that used those re-exports
- tighten storage-surface guard tests so executor production code and manifests
  cannot import storage
- document any intentional dev/test storage exceptions outside executor
- update `engine-crate-map.md` and the main engine consolidation plan

Exit criteria:

- `cargo tree -p strata-executor --edges normal` has no direct
  `strata-storage` edge
- no executor production Rust file imports `strata_storage`
- executor does not publicly re-export storage types
- guard tests fail if a production executor storage import returns

Implementation result:

- `crates/executor/Cargo.toml` no longer has a normal `strata-storage`
  dependency. Executor tests enable engine's `test-support` feature instead of
  importing storage directly.
- `crates/executor/src/lib.rs` no longer re-exports storage `Key` or
  `Namespace`.
- The remaining executor corruption fixture now calls the engine-owned
  `Database::inject_corrupt_json_bytes_for_test` helper, so executor source no
  longer constructs storage keys or namespaces even in `#[cfg(test)]` code.
- The direct-storage guard allowlist is empty. Any production storage import or
  manifest dependency above engine now fails the guard unless a future phase
  adds a new explicit exception.
- `engine-crate-map.md` and the main consolidation plan now describe executor
  as an ordinary engine/intelligence consumer with no direct storage edge.

## Verification

Run targeted checks as each section lands:

```bash
rg -n "strata_storage::|use strata_storage" crates/executor/src -g '*.rs'
rg -n '^strata-storage\\s*=' crates/executor/Cargo.toml
rg -n "TransactionContext|context_mut\\(|Namespace|TypeTag|Key::" crates/executor/src -g '*.rs'
cargo tree -p strata-executor --edges normal --depth 1 | rg "strata-storage"
cargo test -p strata-executor
cargo test -p strata-engine --lib transaction
cargo test -p strata-engine --lib primitives::space
cargo test -p stratadb --test storage_surface_imports
cargo check -p strata-cli
```

Interpretation notes:

- The first three `rg` commands should eventually return no production
  executor matches. Test-only matches should either move to engine tests or be
  documented and guarded.
- The manifest check should return no direct executor dependency on storage.
  The `cargo tree` command may still show transitive storage through engine;
  use it to confirm any remaining storage edge is not direct.
- Root dev/test storage uses are not `EG7` failures unless they become normal
  production dependencies above engine.

## Risks

### Transaction Semantics Drift

The highest-risk part is replacing direct transaction-context use. Executor
transaction commands currently depend on read-your-writes behavior, delete-set
visibility, JSON staged writes, event metadata continuity, and post-commit
side effects. `EG7C` must preserve those behaviors through characterization
tests before moving code.

### Space Delete Partial Cleanup

Current space deletion mixes transactional data deletion with best-effort
runtime cleanup. Moving it into engine should not accidentally make every
best-effort cleanup failure fatal, or accidentally hide failures that used to
abort the command. Preserve the existing policy first; improve it only with an
explicit behavior-change note.

### Public Re-Export Removal

`strata-executor` currently re-exports storage `Key` and `Namespace`. Strata is
pre-v1, so removing that surface is acceptable, but in-repo tests and examples
must be moved to engine-owned types or deleted if they were only exposing
storage internals.

### Accidental Facade Sprawl

Do not create generic pass-through wrappers for every storage operation just
to make executor compile. The new engine APIs should be semantic and should
exist because executor is asking engine to perform a product operation.

## Completion Definition

`EG7` is complete when:

- executor has no normal dependency on storage
- executor production code has no direct storage imports
- executor production code does not use raw storage transaction context as a
  product command runtime
- executor public API no longer exposes storage `Key` or `Namespace`
- storage-origin failures are mapped by engine before executor sees them
- space deletion orchestration is engine-owned
- guard tests enforce the boundary
- executor, engine transaction, engine space, and storage-surface tests pass
