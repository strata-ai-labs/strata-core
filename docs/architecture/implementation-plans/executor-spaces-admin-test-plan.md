# Executor Spaces and Admin Test Plan

## Purpose

Prove that `executor-next` restores space management and safe admin
introspection while preserving the rebuilt architecture: executor is a thin
serialized command boundary, engine owns semantics, storage internals remain
behind engine, and deferred old systems do not leak back into the public
surface.

## Test Matrix

| Area | Cache | Durable Local |
| --- | --- | --- |
| Command serde round-trip | Required | Required |
| Output serde round-trip | Required | Required |
| Space list/create/exists | Required | Required |
| Space delete constraints | Required | Required |
| Space forced delete | Required | Required |
| Branch-local space isolation | Required | Required |
| Reopen persistence | Not applicable | Required |
| Ping/info/health/metrics | Required | Required |
| Describe | Required | Required |
| Config get/get-key | Required | Required |
| Read-only access enforcement | Deferred until executor exposes read-only handles | Deferred until executor exposes read-only handles |
| Error mapping | Required | Required |
| Source guards | Required | Required |

## Contract Tests

### Command JSON Round Trip

- Serialize and deserialize `SpaceList`.
- Serialize and deserialize `SpaceCreate`.
- Serialize and deserialize `SpaceExists`.
- Serialize and deserialize `SpaceDelete` with omitted `force`.
- Serialize and deserialize `SpaceDelete` with `force=true`.
- Serialize and deserialize `Ping`.
- Serialize and deserialize `Info`.
- Serialize and deserialize `Health`.
- Serialize and deserialize `Metrics`.
- Serialize and deserialize `Describe`.
- Serialize and deserialize `ConfigGet`.
- Serialize and deserialize `ConfigureGetKey`.
- Do not include `ConfigureSet`; writable config is deferred.
- Include omitted branch and explicit branch for branch-scoped commands.
- Assert deserialized command equality.

### Output JSON Round Trip

- Round-trip `SpaceList`.
- Round-trip `SpaceCreateResult` for created and already-existing spaces.
- Round-trip `SpaceDeleteResult` for deleted and missing spaces.
- Round-trip `Pong`.
- Round-trip `DatabaseInfo`.
- Round-trip `Health`.
- Round-trip `Metrics`.
- Round-trip `Described`.
- Round-trip `Config`.
- Round-trip `ConfigValue(Some(_))`.
- Round-trip `ConfigValue(None)`.
- Do not include `ConfigSetResult`; writable config is deferred.

### Command Names

- Assert `Command::name()` returns stable names:
  - `space_list`
  - `space_create`
  - `space_exists`
  - `space_delete`
  - `ping`
  - `info`
  - `health`
  - `metrics`
  - `describe`
  - `config_get`
  - `configure_get_key`
  - `configure_set` if included
- The match must stay exhaustive.

### Write Classification

- `SpaceCreate` is a write.
- `SpaceDelete` is a write.
- No writable config command is included in this slice.
- `SpaceList`, `SpaceExists`, `Ping`, `Info`, `Health`, `Metrics`,
  `Describe`, `ConfigGet`, and `ConfigureGetKey` are reads.

## Engine Space Tests

### Default Space Bootstrap

- Open a cache database and list spaces on the default branch.
- Assert `default` exists.
- Assert `_system_` is not returned.
- Open durable-local, close, reopen, and assert the same facts.

### Create Space

- Create a user space.
- Assert `created=true`.
- Assert version and timestamp are present.
- List spaces and assert deterministic ordering.
- Create the same space again.
- Assert `created=false` or documented idempotent no-op.
- Assert no duplicate list entries.

### Space Exists

- Existing `default` returns true.
- Created user space returns true.
- Missing user space returns false.
- `_system_` returns false or invalid input according to the chosen reserved
  space contract, but must not be treated as a normal user space.

### Name Validation

- Reject empty names.
- Reject whitespace-only names.
- Reject names with invalid characters.
- Reject reserved internal names.
- Accept valid names used by existing `ProductSpace`.
- Error codes map to invalid argument, not corruption.

### Branch-Local Isolation

- Create `tenant_a` on `main`.
- Create a second branch.
- Assert the second branch has only its expected inherited/bootstrap spaces.
- Create `tenant_b` on the second branch.
- Assert `tenant_b` does not appear on `main`.
- Fork `main` after creating `tenant_a`.
- Assert the fork sees `tenant_a` when storage inheritance makes it visible.
- Create another space on parent after fork.
- Assert the child does not see the post-fork parent space.

### Durable Reopen

- Create spaces in durable-local mode.
- Close and reopen.
- Assert the created spaces persist.
- Delete a space, close, reopen, and assert it remains absent.

### Delete Constraints

- Reject deleting `default`.
- Reject deleting `_system_`.
- Reject deleting a non-empty user space with `force=false`.
- Return a constraint error with a clear reason.
- Assert data and catalog remain unchanged after rejection.

### Forced Delete Across Primitives

Create one user space with data in every rebuilt primitive:

- KV key
- JSON document
- event entries
- vector collection and vectors
- graph, nodes, edges, and bindings if bindings are space-scoped

Then:

- Execute forced space delete.
- Assert `deleted=true`.
- Assert `deleted_rows` is non-zero.
- Assert `SpaceExists` returns false.
- Assert `SpaceList` omits the space.
- Assert reads in that space fail as missing space or return empty according to
  the final service contract.
- Assert other spaces remain unaffected.
- Assert default space remains usable.

### Index and Artifact Invalidation

- Create a vector collection in a user space.
- Build or testkit-seal vector index artifacts.
- Force delete the space.
- Assert vector index manifest refs for that space are removed or unreachable.
- Assert later recreating the same space cannot reuse stale vector artifacts.
- Assert queries after recreate are exact-correct and empty until new data is
  inserted.

### Delete Failure Atomicity

- Inject a failure before catalog removal.
- Assert data remains visible and the space remains listed.
- Inject a failure after data tombstone planning but before commit.
- Assert no partial delete is externally visible.
- If delete is over budget, assert it fails before any catalog change.

## Executor Space Tests

### Command-To-Engine Wiring

- Execute `Command::SpaceList` through executor.
- Execute `Command::SpaceCreate` through executor.
- Execute `Command::SpaceExists` through executor.
- Execute `Command::SpaceDelete` through executor.
- Assert output variants match the implementation plan.
- Assert default branch resolution matches other executor-next commands.

### Read-Only Access

Deferred until executor-next exposes an explicit read-only handle. This slice
must still keep `Command::is_write()` correct so a future read-only wrapper can
reject `SpaceCreate` and `SpaceDelete` without command-specific inference.

### Error Mapping

- Missing branch maps to executor not-found.
- Invalid space maps to executor invalid-input.
- Non-empty delete without force maps to executor constraint violation.
- Control-plane corruption maps to executor internal/corruption class.

## Engine Admin Tests

### Ping

- `ping` returns the executor/engine package version.
- `ping` does not require a branch.
- `ping` works in cache and durable-local modes.

### Info

- `info` includes target, created/durable facts, default branch, branch count,
  and open state.
- Branch count changes after branch create/delete.
- Space count changes after space create/delete when branch-scoped info is
  requested.
- Durable reopen reports `created=false` on the second open.

### Health

- Healthy database reports `healthy`.
- Missing requested branch reports unhealthy or not-found according to the
  final API contract.
- Injected missing default space catalog row reports unhealthy/corrupt.
- Closed database reports unhealthy or closed-runtime error.
- Health does not mutate database state.

### Metrics

- Metrics returns lightweight open/control facts: target, durable/open state,
  branch count, branch-scoped space count, and control health.
- Storage budget, pressure, and maintenance facts are deferred until storage
  exposes typed diagnostics for them.
- Metrics does not include raw keys, values, vectors, API keys, or secrets.

### Describe

- Empty database describe lists branches and `default` space.
- Describe reports rebuilt primitive capabilities only.
- After writing KV, JSON, event, vector, and graph data, describe reports
  non-zero summaries for those primitives.
- Describe omits old search, recipes, graph ontology, and auto-embed
  capabilities unless those systems are actually restored later.
- Describe does not call old engine modules or executor search handlers.
- Describe handles missing optional primitive summaries by returning zero or an
  explicit degraded fact, not by failing the whole command unless control-plane
  health is corrupt.

### Config Read

- `ConfigGet` returns a sanitized config summary.
- `ConfigGet` does not include API key values.
- `ConfigureGetKey` returns values for allowlisted public keys.
- `ConfigureGetKey` returns `None` or invalid-input for unknown keys according
  to the final contract.
- Secret-like keys return redacted values or are rejected.

### Config Write If Included

- Unknown keys are rejected.
- Open-time-only keys are rejected.
- Values are type-validated.
- Failed writes leave the previous value unchanged.
- Writable settings affect only the documented runtime behavior.
- Secret/model/auto-embed keys remain rejected in this slice.

## Executor Admin Tests

### Command-To-Output Mapping

- `Ping` returns `Output::Pong`.
- `Info` returns `Output::DatabaseInfo`.
- `Health` returns `Output::Health`.
- `Metrics` returns `Output::Metrics`.
- `Describe` returns `Output::Described`.
- `ConfigGet` returns `Output::Config`.
- `ConfigureGetKey` returns `Output::ConfigValue`.
- No `ConfigureSet` or `ConfigSetResult` output exists in this slice.

### Read-Only Access

Deferred until executor-next exposes an explicit read-only handle. The current
coverage requirement is explicit `Command::is_write()` classification for admin
and space commands.

### No State Mutation From Reads

- Capture branch and commit facts.
- Execute every read-only admin command.
- Assert no commit version or branch state changes.
- Assert no space catalog rows are created except required bootstrap rows that
  already occur at database open.

## Source Guards

### Executor Boundary Guards

- `crates/executor-next/src/**` must not import `strata_storage_next`.
- Executor admin/space code must not import `engine-next` persistence modules.
- Executor admin/space code must not reference:
  - `RowClass`
  - `RowAddress`
  - `StorageSpaceId`
  - `CommitPlan`
  - `RowMutation`
  - vector artifact internals
- Executor admin/space code must not scan storage rows directly.

### Deferred Command Guards

- Assert executor-next command vocabulary does not include:
  - `Flush`
  - `Compact`
  - `RetentionApply`
  - `RetentionPreview`
  - `RetentionStats`
  - `AutoEmbedStatus`
  - `EmbedStatus`
  - `ReindexEmbeddings`
  - `ConfigureModel`
  - `Search`
  - recipe commands
  - graph ontology commands

### Secret Guards

- Serialize `ConfigGet` output and assert it does not contain:
  - `openai_api_key`
  - `anthropic_api_key`
  - `google_api_key`
  - `api_key`
  - known test secret values
- Serialize `Describe`, `Info`, `Health`, and `Metrics` and run the same
  assertions.

### Reserved Space Guards

- `SpaceList` never returns `_system_`.
- `SpaceCreate` rejects `_system_`.
- `SpaceDelete` rejects `_system_`.
- Forced delete cannot remove internal system rows.

## Regression Tests

### Default Space Still Works

- After adding space commands, all existing KV/JSON/vector/event/graph tests
  that omit `space` still use `default`.
- Creating and deleting other spaces does not affect default-space data.

### Branch Fork Interactions

- Create a user space and data on parent.
- Fork child.
- Assert child sees inherited space/data according to branch visibility.
- Force delete the space on child.
- Assert parent still sees its space/data.
- Force delete the space on parent after fork.
- Assert child remains isolated from post-fork parent deletion.

### Durable Reopen Interactions

- Create spaces and data.
- Run admin describe.
- Close/reopen.
- Run admin describe again.
- Assert stable branch, space, and primitive summaries.

## Required Commands

Run at minimum:

```sh
cargo test -p strata-engine-next --features testkit space
cargo test -p strata-engine-next --features testkit admin
cargo test -p strata-executor-next --features testkit space
cargo test -p strata-executor-next --features testkit admin
cargo test -p strata-executor-next --features testkit command_roundtrip
cargo clippy -p strata-engine-next -p strata-executor-next --all-targets --features testkit -- -D warnings
```

If the workspace uses `cargo hack` for feature sweeps, add or extend the
existing executor-next and engine-next feature matrix to include these tests.

## Acceptance Criteria

1. Spaces are available through executor-next with list/create/exists/delete.
2. Admin status commands are available through executor-next.
3. Config read commands are available and sanitized.
4. No deferred command families are accidentally restored.
5. Executor remains a thin boundary over engine services.
6. Cache and durable-local modes pass the same behavioral tests, with reopen
   tests covering durable persistence.
7. Space deletion cannot partially remove user data or expose stale vector
   indexes.
8. Admin read commands do not mutate committed state.
