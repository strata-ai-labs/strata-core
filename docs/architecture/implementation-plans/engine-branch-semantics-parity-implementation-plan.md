# Engine Branch Semantics Parity Implementation Plan

Status: draft implementation plan

Test plan:
`docs/architecture/implementation-plans/engine-branch-semantics-parity-test-plan.md`

## Problem

The rebuilt engine has a useful branch and KV vertical spine, but its branch
semantics are still much thinner than the old engine. Current branch support can
bootstrap the default branch, list/lookup catalog entries, and create a branch
from the current source head. That is not enough for the next product layer:
KV, JSON, event, vector, and graph all need the same branch identity, fork
point, lifecycle, visibility, and generation-fencing rules.

This slice restores the branch semantics that are required before adding more
product primitives or public executor branch commands. Merge, publish, review,
cherry-pick, revert, restore, tags, and notes remain deliberately deferred.

## Investigation Evidence

The old engine evidence below is used to recover product invariants and known
failure cases. It is not an instruction to port the old implementation shape.
The rebuilt engine should use the storage branch API, engine persistence
adapter, control-plane row model, and generation-fenced commits that fit the
current architecture.

Old engine evidence:

- `crates/engine/src/database/branch_service.rs`
- `crates/engine/src/database/branch_mutation.rs`
- `crates/engine/src/database/open.rs`
- `crates/engine/src/database/open_options.rs`
- `crates/engine/src/primitives/branch/index.rs`
- `crates/engine/src/branch_domain/branch.rs`
- `crates/engine/src/branch_ops/mod.rs`
- `crates/engine/src/branch_ops/branch_control_store.rs`
- `crates/engine/tests/branch_id_characterization.rs`
- `crates/engine/tests/branch_isolation_tests.rs`
- `crates/engine/tests/recovery_tests.rs`
- `crates/engine/src/primitives/branch/index.rs` unit tests

Current rebuilt targets:

- `crates/engine-next/src/branch/`
- `crates/engine-next/src/control/`
- `crates/engine-next/src/api/branch.rs`
- `crates/engine-next/src/api/database.rs`
- `crates/engine-next/src/data/kv/service.rs`
- `crates/engine-next/src/persistence/`
- `crates/engine-next/tests/branch_and_kv.rs`
- `crates/engine-next/tests/control_plane.rs`
- `crates/executor-next/`

Storage API evidence:

- `crates/storage-next/src/api/branch.rs`
- `docs/architecture/implementation-plans/M4/L9/l9e-branch-lifecycle-api-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9e-branch-lifecycle-api-test-plan.md`

## Investigation Findings

1. The old branch id derivation is byte-stable and explicitly tested:
   `"default"` maps to the nil UUID, UUID strings pass through verbatim, and
   ordinary names use the locked UUID-v5 namespace from `strata-core`.
2. Current rebuilt branch ids are not parity-compatible. `BranchCatalogRecord`
   derives ids with a new SHA-256 scheme and uses `[0x01; 16]` for the default
   branch. That must be corrected before branch durability can be trusted as an
   old-engine-compatible model.
3. The old engine distinguishes root branch creation from forked branch
   creation. `BranchService::create` creates an independent branch with no fork
   anchor; `BranchService::fork` creates a copy-on-write branch from a source
   branch and records the fork point.
4. Current rebuilt `create_from_head` is semantically a fork-current operation,
   but its public summary only reports name and generation. It drops source
   name, source generation, fork version, fork timestamp, branch id, and
   lifecycle state.
5. The old engine has generation-aware lifecycle identity:
   `(branch_id, generation)` identifies a branch lifecycle instance, same-name
   delete/recreate increments generation, and tombstoned records remain so the
   next generation is monotonic.
6. The old engine protects same-name races with active-pointer/OCC machinery.
   The invariant is still required: at most one live lifecycle instance per
   name, monotonic generation after recreate, and stale writers fail. The
   implementation does not need to reuse active pointers if the current storage
   generation guards and engine control-plane commits can enforce the same
   property cleanly.
7. The old engine gates writes through branch lifecycle. Deleted branches are
   not writable, archived branches are visible but not writable, and generation
   changes abort stale writes. Current KV writes already pass
   `expected_generation`, which is the right hook, but branch lifecycle needs
   to drive it consistently.
8. The old engine persists the effective default branch marker and rejects
   incompatible primary reuse. Current rebuilt open options do not yet expose
   a product default-branch selection.
9. The old fork path has strict failure ordering: storage fork first, then
   metadata/control activation, with rollback or fail-closed behavior. The
   rebuilt engine already has pending branch create rows; this should be
   generalized into a branch operation ledger rather than replaced with a
   second ad hoc path.
10. Storage already exposes the mechanics needed for this parity slice:
    create, describe, list, fork-current, fork-at-version, fork-at-timestamp,
    clear, delete, generation guards, parent summaries, and cleanup facts.
    The engine should wrap those through `persistence`, not duplicate storage
    mechanics above it.

## Scope

In scope:

1. Old-compatible branch-name validation and branch-id derivation.
2. Engine-owned product branch summaries with id, generation, lifecycle, parent,
   fork point, and timestamps where available.
3. Root branch creation.
4. Fork-current branch creation from a source branch.
5. Fork-at-version and fork-at-timestamp if the storage API exposes retained
   history in the current target.
6. Branch delete with generation guard and cleanup facts.
7. Same-name delete/recreate generation monotonicity.
8. Default-branch open metadata for cache and durable-local primary opens.
9. KV visibility and generation fencing across branch create, fork, delete, and
   recreate.
10. Durable reopen of branch catalog, parent facts, lifecycle state, and default
    branch metadata.
11. Source/dependency guards that keep storage types behind persistence and keep
    deferred branch workflow vocabulary out of the public branch API.

Out of scope:

1. Merge.
2. Publish and review.
3. Cherry-pick.
4. Revert.
5. Restore.
6. Branch diff.
7. Tags and notes.
8. Follower refresh behavior.
9. Cross-primitive merge handlers.
10. Public manual transaction sessions.

## Binding Decisions

1. **Invariants matter more than old mechanisms.** Preserve old observable
   behavior where it is load-bearing: identity stability, one live branch per
   name, fork visibility, delete/recreate generation monotonicity, stale-write
   rejection, and durable reopen behavior. Do not port old internal structures
   merely because they existed.
2. **Branch id parity is non-negotiable.** Product branch names must resolve the
   way the old engine resolves them: literal `"default"` is nil UUID, UUID
   spellings pass through, and other names use the old UUID-v5 namespace. The
   current SHA-256 derivation is a compatibility bug to remove.
3. **Engine owns product branch semantics.** Storage branch APIs provide
   mechanics over opaque ids. Engine owns names, default branch policy,
   lifecycle, same-name generation rules, and executor-facing summaries.
4. **Storage remains the branch mechanics authority.** Fork visibility,
   retained-history fork, generation-guarded delete, clear, and cleanup facts
   come from storage through `StoragePersistence`.
5. **Do not create a second semantic path.** Branch catalog updates, KV
   generation fencing, and executor commands must all use the same engine
   branch service and persistence adapter.
6. **Root create and fork are distinct.** A root branch has no parent and starts
   empty. A forked branch inherits source visibility at a specific source
   frontier. Current `create_from_head` should either be renamed or kept as a
   compatibility alias over the explicit fork-current operation.
7. **Lifecycle identity is generation-aware.** Same branch name can have
   multiple historical lifecycle instances, but only one live instance.
   Deleting and recreating a name increments generation and must not inherit
   old data.
8. **Deleted branch data must not resurrect.** Writes that started before
   delete and commit after delete must fail through generation/lifecycle
   fencing.
9. **Default branch is protected.** The configured default branch cannot be
   deleted in this slice. Deleting the last live product branch also fails.
10. **Cache and durable-local share semantics.** Cache is volatile but must obey
   the same branch lifecycle and KV visibility rules. Durable-local additionally
   proves close/reopen persistence.
11. **Deferred workflows stay absent.** Public branch APIs must not expose merge,
    publish, review, cherry-pick, revert, restore, tag, or note methods until
    those slices have their own plans.

## Target Public Engine API

The exact Rust shape can follow existing crate patterns, but the branch service
should expose this semantic surface:

1. `branches().list() -> EngineResult<Vec<BranchSummary>>`
2. `branches().get(name) -> EngineResult<BranchSummary>`
3. `branches().create(name) -> EngineResult<BranchCreateOutcome>`
4. `branches().fork_current(source, name) -> EngineResult<BranchCreateOutcome>`
5. `branches().fork_at_version(source, name, version) -> EngineResult<BranchCreateOutcome>`
6. `branches().fork_at_timestamp(source, name, timestamp) -> EngineResult<BranchCreateOutcome>`
7. `branches().delete(name) -> EngineResult<BranchDeleteOutcome>`

`create_from_head` may stay temporarily as an alias for `fork_current`, but new
tests and docs should use permanent branch vocabulary.

`BranchSummary` should report:

1. name
2. branch id
3. generation
4. lifecycle status
5. parent branch name, id, and generation when forked
6. fork version and fork timestamp when forked
7. created version/timestamp when available
8. deleted version/timestamp when tombstoned and visible through diagnostics

`BranchDeleteOutcome` should report:

1. deleted branch summary
2. generation before and after when storage reports it
3. cleanup facts from storage
4. whether storage release was protected by pinned reachability

Normal `list` should return only live product branches. Diagnostics can expose
tombstones later if needed.

## Target Internal Model

### Branch Name Validation

Use old public validation unless a stricter decision is explicitly documented:

1. reject empty names
2. reject whitespace-only names
3. reject names longer than 255 bytes
4. reject names starting with `_`
5. reject control bytes
6. reject non-literal names that alias the nil default-branch sentinel
7. accept ordinary slashes and punctuation used by branch names such as
   `feature/abc`
8. keep `"default"` case-sensitive

### Branch Id Derivation

Add one engine-owned helper and use it everywhere branch names become branch ids:

1. `"default"` -> `[0; 16]`
2. UUID string -> parsed UUID bytes
3. any other valid name -> UUID-v5 over the old locked namespace

Add fixture tests for `default`, `main`, `_system_` as a reserved derivation
anchor only, `feature/abc`, and a UUID string. `_system_` should remain rejected
at product boundaries even though the derivation helper can compute its old id
for internal compatibility tests.

### Branch Catalog Record

Replace the minimal current catalog shape with a generation-aware record:

1. name
2. branch id
3. generation
4. lifecycle status: active or deleted for this slice
5. parent: optional source name/id/generation plus fork version/timestamp
6. created version/timestamp
7. deleted version/timestamp
8. state revision or equivalent catalog-row version

Keep a durable per-name next-generation counter, or derive the next generation
from retained tombstone records during load. Choose the simpler approach that
fits the rebuilt control-plane row model. The requirement is monotonic
generation across reopen and same-name recreate, not a specific counter layout.

### Branch Operation Ledger

Use the existing control-plane pending-row pattern for branch operations that
span storage branch mechanics and catalog activation. This can be a generalized
pending branch operation ledger or a smaller per-operation marker set. The
choice should minimize moving parts while preserving fail-closed behavior.

The pending state must cover:

1. pending root create
2. pending fork-current
3. pending fork-at-version
4. pending fork-at-timestamp
5. pending delete

For the first implementation, pending rows may fail closed on reopen with a
stable corruption error. Repair can be a later enhancement. The hard invariant
is that a half-created or half-deleted branch is never silently exposed as
healthy.

### KV Generation Fencing

Every KV write already commits with `expected_generation`. Keep that invariant
and make branch lifecycle the single source of that generation. Reads should
resolve the selected branch through the catalog before touching persistence.
Writes should reject if the live generation changed after service creation or
before commit. Prefer this generation-fenced storage commit path over old
branch commit locks unless tests prove an uncovered race.

## Implementation Order

### 1. Branch Identity Parity

Files:

1. `crates/engine-next/src/branch/name.rs`
2. `crates/engine-next/src/branch/catalog.rs`
3. `crates/engine-next/tests/branch_semantics.rs`
4. `crates/engine-next/tests/dependency_guards.rs`

Tasks:

1. Add old-compatible branch id derivation.
2. Replace the current SHA-256 derivation.
3. Make the default branch id the nil UUID.
4. Add old branch-id fixture tests.
5. Add validation tests for empty, whitespace-only, too-long, reserved prefix,
   control bytes, default sentinel aliases, UUID names, and case sensitivity.
6. Add a source guard that rejects SHA-256 branch-id derivation in branch
   catalog code.

Exit gate:

1. Current default branch data is written under the old nil branch id.
2. Branch id fixture tests match the old engine characterization anchors.

### 2. Catalog Record Expansion

Files:

1. `crates/engine-next/src/branch/catalog.rs`
2. `crates/engine-next/src/control/records.rs`
3. `crates/engine-next/src/control/bootstrap.rs`
4. `crates/engine-next/src/api/branch.rs`
5. `crates/engine-next/tests/control_plane.rs`

Tasks:

1. Add lifecycle and parent/fork fields to engine catalog records.
2. Version the control-plane branch record payload.
3. Persist and decode next-generation state or tombstone records.
4. Keep active branch index deterministic and sorted.
5. Fail closed on missing active branch records, duplicate names, corrupt parent
   facts, or unsupported payload versions.

Exit gate:

1. Existing branch list/get behavior still works.
2. Durable reopen preserves branch id, generation, lifecycle, and parent facts.

### 3. Persistence Branch Adapter

Files:

1. `crates/engine-next/src/persistence/adapter.rs`
2. `crates/engine-next/src/persistence/mod.rs`
3. `crates/engine-next/tests/persistence_adapter.rs`

Tasks:

1. Add engine-owned wrappers for storage branch describe/list.
2. Add wrappers for create, fork-current, fork-at-version, fork-at-timestamp,
   and delete.
3. Map storage generation mismatch to stable engine conflict errors.
4. Map storage history-unavailable fork errors to stable branch history errors.
5. Preserve storage cleanup facts in engine-owned DTOs.
6. Keep all storage branch request/outcome types private to persistence.

Exit gate:

1. Branch APIs outside persistence do not import storage branch types.
2. Persistence tests prove every storage branch error class maps to an engine
   class/code without leaking storage type names.

### 4. Root Create And Fork-Current

Files:

1. `crates/engine-next/src/branch/service.rs`
2. `crates/engine-next/src/control/bootstrap.rs`
3. `crates/engine-next/src/api/branch.rs`
4. `crates/engine-next/tests/branch_semantics.rs`
5. `crates/engine-next/tests/branch_and_kv.rs`

Tasks:

1. Implement root `create(name)` with no parent.
2. Implement `fork_current(source, name)` with source name/id/generation and
   storage fork facts.
3. Keep `create_from_head` as an alias only if needed by existing callers.
4. Reject duplicate destination names before storage mutation.
5. Reject missing or deleted sources.
6. Persist operation ledger rows before storage mutation and clear them only
   after catalog activation.
7. Roll back or fail closed if storage succeeds but catalog activation fails.

Exit gate:

1. Root-created branches start empty.
2. Fork-current branches inherit source visibility.
3. Source and child writes remain isolated after fork.
4. Duplicate and missing-source errors are stable.

### 5. Historical Fork

Files:

1. `crates/engine-next/src/branch/service.rs`
2. `crates/engine-next/src/persistence/adapter.rs`
3. `crates/engine-next/tests/branch_semantics.rs`

Tasks:

1. Implement fork-at-version through storage.
2. Implement fork-at-timestamp through storage timeline resolution.
3. Store fork version/timestamp in the branch summary.
4. Reject unretained history with stable errors.
5. Ensure fork-at-version uses retained watermark semantics, not exact-row-only
   semantics.

Exit gate:

1. Reads on the child reflect the requested historical source frontier.
2. Latest source writes after the requested fork point are not visible in the
   child.

### 6. Delete And Recreate

Files:

1. `crates/engine-next/src/branch/service.rs`
2. `crates/engine-next/src/control/records.rs`
3. `crates/engine-next/tests/branch_semantics.rs`
4. `crates/engine-next/tests/branch_and_kv.rs`

Tasks:

1. Implement delete with generation guard.
2. Reject deleting the configured default branch.
3. Reject deleting the last live product branch.
4. Remove deleted branches from normal list/get.
5. Preserve tombstone/generation state for same-name recreate.
6. Ensure same-name recreate increments generation.
7. Ensure recreated branches do not inherit deleted branch data.
8. Preserve and surface cleanup facts from storage.

Exit gate:

1. Deleted branches cannot be opened for KV reads or writes.
2. Stale writes after delete fail by generation/lifecycle fencing.
3. Recreate is generation-monotonic and data-clean.

### 7. Default Branch Open Semantics

Files:

1. `crates/engine-next/src/api/options.rs`
2. `crates/engine-next/src/api/database.rs`
3. `crates/engine-next/src/control/bootstrap.rs`
4. `crates/engine-next/tests/control_plane.rs`

Tasks:

1. Add explicit default-branch option to cache and durable-local primary opens.
2. Persist the effective default branch in control-plane metadata.
3. Create the default branch when opening a new database.
4. On durable reopen, prefer persisted default metadata over a conflicting
   request and reject or report incompatible reuse according to the final API
   decision.
5. Keep follower behavior deferred.

Exit gate:

1. Cache open with default branch creates that branch.
2. Durable reopen preserves the persisted default branch.
3. The configured default branch cannot be deleted.

### 8. Executor Branch Command Surface

Files:

1. `crates/executor-next/src/command.rs`
2. `crates/executor-next/src/output.rs`
3. `crates/executor-next/src/executor.rs`
4. `crates/executor-next/tests/command_contract.rs`
5. `crates/executor-next/tests/branch_behavior.rs`

Tasks:

1. Add serialized commands for list, describe, root create, fork-current,
   fork-at-version, fork-at-timestamp, and delete.
2. Delegate every branch command through engine branch APIs.
3. Add outputs for branch summaries and delete cleanup facts.
4. Keep executor as a stateless delegator.
5. Do not add merge, publish, review, cherry-pick, revert, restore, tag, or note
   command variants in this slice.

Exit gate:

1. Branch commands round-trip through JSON.
2. Executor branch behavior matches engine branch behavior.
3. Source guards prove no lower-layer branch mechanics are implemented in
   executor.

## Test Plan

### Branch Identity Tests

1. `branch_id_default_is_nil_uuid`
2. `branch_id_uuid_string_passes_through`
3. `branch_id_uuid_case_and_hyphenation_match`
4. `branch_id_common_names_match_old_anchors`
5. `branch_id_empty_string_is_rejected_at_product_boundary`
6. `branch_name_rejects_default_sentinel_alias`
7. `branch_name_rejects_reserved_prefix`
8. `branch_name_rejects_control_bytes`
9. `branch_name_rejects_whitespace_only`
10. `branch_name_rejects_over_255_bytes`

### Catalog And Reopen Tests

1. `cache_open_bootstraps_default_branch_with_nil_id`
2. `durable_open_reopen_preserves_branch_catalog`
3. `durable_open_reopen_preserves_parent_fork_facts`
4. `durable_open_reopen_preserves_default_branch_marker`
5. `corrupt_branch_catalog_payload_fails_closed`
6. `missing_branch_catalog_row_fails_closed`
7. `pending_branch_operation_fails_closed_on_reopen`
8. `branch_list_is_sorted_and_excludes_deleted`
9. `branch_get_missing_returns_not_found`
10. `branch_summary_reports_id_generation_lifecycle_and_parent`

### Root Create And Fork Tests

1. `branch_create_root_starts_empty`
2. `branch_create_duplicate_rejects`
3. `branch_fork_current_inherits_source_visible_rows`
4. `branch_fork_current_isolates_child_writes`
5. `branch_fork_current_isolates_source_writes_after_fork`
6. `branch_fork_missing_source_rejects`
7. `branch_fork_duplicate_destination_rejects`
8. `branch_fork_records_source_name_generation_and_version`
9. `branch_fork_after_close_rejects`
10. `branch_fork_source_deleted_rejects`

### Historical Fork Tests

1. `branch_fork_at_retained_version_uses_requested_frontier`
2. `branch_fork_at_between_commits_uses_retained_watermark`
3. `branch_fork_at_unretained_version_rejects`
4. `branch_fork_at_timestamp_resolves_timeline`
5. `branch_fork_at_unretained_timestamp_rejects`
6. `branch_fork_at_history_records_fork_metadata`

### Delete And Recreate Tests

1. `branch_delete_removes_from_list`
2. `branch_delete_unknown_rejects`
3. `branch_delete_default_rejects`
4. `branch_delete_last_live_branch_rejects`
5. `branch_delete_reports_cleanup_facts`
6. `branch_delete_generation_mismatch_rejects`
7. `branch_delete_blocks_late_stale_write`
8. `branch_delete_clears_visible_kv_rows`
9. `branch_recreate_increments_generation`
10. `branch_recreate_does_not_inherit_deleted_data`
11. `branch_recreate_durable_reopen_preserves_generation`

### KV Integration Tests

1. `kv_resolves_branch_through_catalog_before_read`
2. `kv_resolves_branch_through_catalog_before_write`
3. `kv_put_on_deleted_branch_rejects`
4. `kv_get_on_deleted_branch_rejects`
5. `kv_stale_generation_commit_rejects`
6. `kv_fork_current_read_equivalence_point_scan_history`
7. `kv_fork_at_version_read_equivalence_point_scan_history`
8. `kv_branch_delete_and_recreate_keeps_histories_separate`

### Executor Tests

1. `branch_commands_round_trip_through_json`
2. `branch_command_names_are_exhaustive`
3. `executor_branch_list_delegates_to_engine`
4. `executor_branch_describe_delegates_to_engine`
5. `executor_branch_create_delegates_to_engine`
6. `executor_branch_fork_current_delegates_to_engine`
7. `executor_branch_delete_delegates_to_engine`
8. `executor_branch_errors_are_executor_shaped`

### Source Guards

1. `engine_branch_catalog_does_not_use_sha256_derivation`
2. `engine_public_branch_api_has_no_storage_types`
3. `engine_public_branch_api_has_no_merge_method`
4. `engine_public_branch_api_has_no_publish_review_method`
5. `engine_public_branch_api_has_no_cherry_pick_revert_restore_methods`
6. `executor_branch_handlers_do_not_import_storage`
7. `executor_branch_helpers_delegate_through_execute_command`
8. `planning_labels_do_not_appear_in_code_or_tests`

## Verification

Targeted checks after each implementation step:

```bash
cargo fmt --all
cargo test -p strata-engine-next --all-features --test branch_semantics
cargo test -p strata-engine-next --all-features --test branch_and_kv
cargo test -p strata-engine-next --all-features --test control_plane
cargo test -p strata-engine-next --all-features --test persistence_adapter
cargo clippy -p strata-engine-next --all-features --all-targets -- -D warnings
```

After executor branch commands land:

```bash
cargo test -p strata-executor-next --all-features
cargo clippy -p strata-executor-next --all-features --all-targets -- -D warnings
```

Closeout checks:

```bash
cargo test -p strata-engine-next --all-features
cargo test -p strata-executor-next --all-features
```

## Stop Conditions

Stop and revise the plan if any of these are true:

1. Storage fork-current in cache cannot preserve source visibility without a
   new storage mechanic.
2. Old-compatible nil default branch id collides with rebuilt control-plane
   system branch assumptions in a way that cannot be isolated by row class.
3. Generation-guarded KV commits can still land after branch delete/recreate.
4. Pending operation fail-closed behavior would strand normal durable reopen
   after an ordinary clean close.
5. Historical fork APIs do not expose enough fork metadata to populate engine
   branch summaries without guessing.

## Non-Goals Ledger

The following are explicitly left for later plans:

1. Branch merge conflict semantics.
2. Branch diff and three-way diff.
3. Publish/review workflows.
4. Cherry-pick, revert, and restore.
5. Branch tags and notes.
6. Follower default-branch refresh semantics.
7. Cross-primitive merge handlers.
