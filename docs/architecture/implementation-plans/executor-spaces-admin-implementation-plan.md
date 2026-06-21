# Executor Spaces and Admin Implementation Plan

## Problem

`executor-next` has rebuilt the primitive command surface, but it still lacks
the old executor's space management and basic database administration commands.
Those commands are important for first-run setup, SDK/CLI introspection, agent
workflows, and operational checks.

This plan restores only Spaces and Admin. Transactions, search, recipes, graph
ontology, branch diff/merge, tags, notes, retention commands, model
configuration, auto-embedding, and low-level storage maintenance remain
deferred.

## Old Evidence

- `crates/executor/src/command.rs`
- `crates/executor/src/output.rs`
- `crates/executor/src/types.rs`
- `crates/executor/src/executor.rs`
- `crates/executor/src/handlers/space.rs`
- `crates/executor/src/handlers/space_delete.rs`
- `crates/executor/src/handlers/database.rs`
- `crates/executor/src/handlers/config.rs`
- `crates/executor/src/handlers/maintenance.rs`
- `crates/executor/src/bridge.rs`

## Current Targets

- `crates/engine-next/src/api/space.rs`
- `crates/engine-next/src/api/admin.rs`
- `crates/engine-next/src/api/database.rs`
- `crates/engine-next/src/control/space.rs`
- `crates/engine-next/src/persistence/adapter.rs`
- `crates/executor-next/src/command.rs`
- `crates/executor-next/src/output.rs`
- `crates/executor-next/src/types.rs`
- `crates/executor-next/src/executor.rs`
- `crates/executor-next/tests/`
- `crates/engine-next/tests/`

## Scope

### Restore Now

Restore these old executor command families:

| Family | Commands |
| --- | --- |
| Spaces | `SpaceList`, `SpaceCreate`, `SpaceExists`, `SpaceDelete` |
| Admin status | `Ping`, `Info`, `Health`, `Metrics`, `Describe` |
| Admin config read | `ConfigGet`, `ConfigureGetKey` |

Do not add `ConfigureSet` in this slice. The restored admin surface is
read-only except for explicit space management commands. Runtime configuration
changes should wait for a typed profile/init surface instead of reintroducing
the old catch-all mutable `StrataConfig` API.

### Defer

Do not restore these as part of this plan:

- `Flush`
- `Compact`
- `TimeRange`
- `DurabilityCounters`
- `ConfigSetAutoEmbed`
- `AutoEmbedStatus`
- `EmbedStatus`
- `ReindexEmbeddings`
- `ConfigureModel`
- model provider/API key mutation
- retention commands
- transaction commands
- search and recipe commands
- graph ontology or graph analytics commands

## Design Decisions

1. **Executor remains a thin boundary.** Executor validates command shape,
   resolves branch defaults, converts wire types, maps errors, and shapes
   outputs. Engine owns space catalog and admin semantics.

2. **Spaces are branch-local control-plane data.** A space registered on one
   product branch does not automatically appear on another branch except when
   branch fork materialization makes it visible through storage inheritance.

3. **Default and system spaces are reserved.** `default` must always exist and
   cannot be deleted. `_system_` is internal and must never appear in the
   user-facing `SpaceList`.

4. **Space creation is idempotent.** Creating an existing user space returns a
   successful no-op outcome rather than corrupting the catalog or duplicating
   rows.

5. **Space deletion is conservative.** Deleting a non-empty user space without
   `force=true` returns a constraint error. Forced delete tombstones all visible
   data in that branch and space across KV, JSON, vector, event, graph, and
   relevant primitive indexes before removing the catalog entry.

6. **No low-level maintenance commands.** `Flush` and `Compact` are storage
   implementation controls, not product-facing admin commands. Diagnostics may
   report maintenance state, but user commands should not expose manual
   compaction or flush knobs in this slice.

7. **Admin config is safe and explicit.** `ConfigGet` returns a sanitized
   runtime/open summary. `ConfigureGetKey` reads only known public keys.
   `ConfigureSet` is omitted in this slice. No API keys or local model choices
   are changed here.

8. **Health is fail-closed.** Control-plane corruption or missing required
   rows must surface as degraded/unhealthy diagnostics. Data commands should
   continue to use existing `require_healthy` behavior.

9. **Describe is an introspection snapshot, not search.** It may count/list
   primitive summaries through public engine services, but it must not invoke
   search, recipes, intelligence, or shadow vector systems.

10. **Command names preserve old compatibility.** Use the old Rust variants
    where possible: `Ping`, `Info`, `Health`, `Metrics`, `Describe`,
    `ConfigGet`, `ConfigureGetKey`, `SpaceList`, `SpaceCreate`,
    `SpaceExists`, and `SpaceDelete`. The JSON command names remain
    snake_case through the executor-next serde policy.

## Required Engine Surface

Add public engine-next DTOs and services before wiring executor commands.

### Space API

Add `Database::spaces(branch: BranchName) -> EngineResult<SpaceService<'_>>`.

Add `SpaceService` methods:

- `list() -> EngineResult<Vec<ProductSpace>>`
- `create(space: ProductSpace) -> EngineResult<SpaceCreateOutcome>`
- `exists(space: &ProductSpace) -> EngineResult<bool>`
- `delete(space: &ProductSpace, force: bool) -> EngineResult<SpaceDeleteOutcome>`

Add DTOs:

- `SpaceCreateOutcome`
  - `space`
  - `created`
  - `version`
  - `timestamp`
- `SpaceDeleteOutcome`
  - `space`
  - `deleted`
  - `force`
  - `deleted_rows`
  - `version`
  - `timestamp`
- `SpaceUsageSummary`
  - `space`
  - `kv_count`
  - `json_count`
  - `vector_collection_count`
  - `vector_entry_count`
  - `event_count`
  - `graph_count`
  - `graph_node_count`
  - `graph_edge_count`

`SpaceUsageSummary` is required for delete constraints and useful for
`Describe`, but it does not need to be exposed as a standalone command in the
first executor slice.

### Admin API

Add `Database::admin() -> EngineResult<AdminService<'_>>` or equivalent
database methods that return typed snapshots.

Add `AdminService` methods:

- `ping() -> EngineResult<PingSummary>`
- `info() -> EngineResult<DatabaseInfoSummary>`
- `health(branch: Option<&BranchName>) -> EngineResult<HealthSummary>`
- `metrics(branch: Option<&BranchName>) -> EngineResult<MetricsSummary>`
- `describe(branch: Option<&BranchName>) -> EngineResult<DescribeSummary>`
- `config() -> EngineResult<AdminConfigSummary>`
- `config_value(key: &str) -> EngineResult<Option<String>>`
- optional: `set_config_value(key: &str, value: &str) -> EngineResult<ConfigSetOutcome>`

Admin summaries should be engine DTOs. Executor must not assemble admin
responses by scanning storage directly.

## Public Executor Command Set

| Command | Inputs | Output |
| --- | --- | --- |
| `SpaceList` | branch? | `SpaceList` |
| `SpaceCreate` | branch?, space | `SpaceCreateResult` |
| `SpaceExists` | branch?, space | `Bool` |
| `SpaceDelete` | branch?, space, force? | `SpaceDeleteResult` |
| `Ping` | none | `Pong` |
| `Info` | branch? | `DatabaseInfo` |
| `Health` | branch? | `Health` |
| `Metrics` | branch? | `Metrics` |
| `Describe` | branch? | `Described` |
| `ConfigGet` | none | `Config` |
| `ConfigureGetKey` | key | `ConfigValue` |
No writable config command is restored in this slice.

## Output Variants

Add executor-next output variants:

- `SpaceList(Vec<String>)`
- `SpaceCreateResult { space, created, version, timestamp }`
- `SpaceDeleteResult { space, deleted, force, deleted_rows, version, timestamp }`
- `Pong { version }`
- `DatabaseInfo(AdminDatabaseInfo)`
- `Health(AdminHealth)`
- `Metrics(AdminMetrics)`
- `Described(AdminDescribe)`
- `Config(AdminConfig)`
- `ConfigValue(Option<String>)`
- No `ConfigSetResult` output is added because writable config is deferred.

Add serializable helper types under `executor-next/src/types.rs` that mirror the
engine DTOs without exposing engine internals or secrets.

## Admin Data Shape

### `AdminDatabaseInfo`

Required fields:

- `version`
- `target`
- `created`
- `durable`
- `default_branch`
- `branch_count`
- `space_count`
- `open`

Optional fields:

- `path`
- `uptime_secs` if engine records an open instant
- `active_readers` if storage diagnostics expose it

### `AdminHealth`

Required fields:

- `status`: `healthy`, `degraded`, or `unhealthy`
- `control_plane`
- `storage`
- `default_branch`
- `space_catalog`

`status` is the worst subsystem state. Control-plane corruption is unhealthy.
Missing requested branch is unhealthy for branch-scoped health.

### `AdminMetrics`

Keep this modest and diagnostic:

- `target`
- `branch_count`
- `space_count`
- `budget`
- `storage_pressure`
- `source_layout`
- `maintenance`
- `wal_growth`

Use storage diagnostics where available. Do not expose raw private paths,
secrets, or large per-key/per-row payloads.

### `AdminDescribe`

Required fields:

- `version`
- `target`
- `default_branch`
- `branch`
- `branches`
- `spaces`
- `primitives`
- `config`
- `capabilities`

Primitive summary should cover the rebuilt primitives only:

- KV count for the default or requested space
- JSON document count for the default or requested space
- Event count for the default or requested space
- Vector collection summaries for the default or requested space
- Graph summaries for the default or requested space

Capabilities should reflect what exists in `*-next`:

- `kv`
- `json`
- `event`
- `vector`
- `vector_index`
- `graph_core`
- `arrow`
- `inference` when compiled

Do not report old search, recipes, graph ontology, or auto-embedding as active.

## Implementation Slices

### 1. Engine Space Service

1. Add `api/space.rs` DTOs.
2. Expose `Database::spaces`.
3. Move user-facing list/create/exists behavior through `SpaceService`.
4. Reuse `control::space::registration_mutations`.
5. Add internal helpers for reading branch-local space indexes.
6. Enforce reserved space rules.

Exit criteria:

1. `default` appears on every initialized branch.
2. `_system_` never appears in user-facing lists.
3. Create is idempotent.
4. Missing branch maps to not-found.

### 2. Engine Space Delete

1. Add space usage scan helpers by row class.
2. Reject deleting `default` and `_system_`.
3. Reject non-empty user-space delete unless `force=true`.
4. Implement forced delete by tombstoning visible rows across primitive row
   classes for that product space.
5. Include vector index manifests/artifacts in the invalidation story: forced
   space delete must remove branch-local vector index manifest refs for that
   space and make stale artifacts unreachable.
6. Commit data tombstones and catalog removal atomically when possible.
7. If atomic delete exceeds commit budget, fail cleanly before partial user
   space removal; do not silently partially delete.

Exit criteria:

1. Forced delete removes visible data from all rebuilt primitives.
2. Failed delete leaves catalog and data unchanged.
3. Catalog corruption fails closed.

### 3. Engine Admin Service

1. Add `api/admin.rs` DTOs.
2. Add `Database::admin` or direct database methods.
3. Build `Ping`, `Info`, `Health`, and `Metrics` from open summary,
   control diagnostics, and storage diagnostics.
4. Build `Describe` through public engine services.
5. Build `ConfigGet` from explicit engine/open/runtime settings.
6. Build `ConfigureGetKey` from a fixed allowlist.
7. Omit `ConfigureSet`; writable runtime profile/configuration belongs to a
   later typed init/profile API.

Exit criteria:

1. Admin commands work in cache and durable-local modes.
2. No admin command imports or reads storage internals from executor.
3. No secret values are returned.
4. Deferred systems are not reported as active.

### 4. Executor Commands and Outputs

1. Add command variants.
2. Add output variants and wire types.
3. Add `Command::name` cases.
4. Add `Command::is_write` cases for `SpaceCreate` and `SpaceDelete`.
5. Wire executor dispatch to engine services.
6. Preserve default branch behavior.
7. Keep space commands branch-scoped and not space-scoped.
8. Add convenience methods only if they execute through `Command`.

Exit criteria:

1. Every restored command round-trips through JSON.
2. Write classification is explicit for the restored commands.
3. Error mapping is stable and documented.

### 5. Documentation and Compatibility Notes

1. Update command parity documentation if present.
2. Add examples for:
   - `space_list`
   - `space_create`
   - `space_delete` with and without force
   - `ping`
   - `info`
   - `health`
   - `describe`
   - `config_get`
3. Note that `flush`, `compact`, retention, search, recipes, model config, and
   auto-embed are intentionally deferred.

Exit criteria:

1. Users can discover the restored commands from executor docs.
2. Deferred old commands are not ambiguous.

## Source Guards

Add guard tests that enforce:

- executor-next admin/space code does not import `strata_storage_next`
- executor-next admin/space code does not import engine persistence modules
- executor-next does not scan row classes directly
- executor-next does not reference vector artifact internals
- executor-next does not expose `_system_`
- executor-next does not expose secret-like config keys or values
- admin code does not call old search, recipes, graph ontology, or intelligence
  modules
- `Flush` and `Compact` are not restored in executor-next command vocabulary

## Open Questions

1. Should `SpaceDelete(force=true)` support chunked deletion for very large
   spaces, or should the first slice reject over-budget deletes and leave
   chunking to a later maintenance API?
2. Should `Describe` summarize all spaces or only the selected/default space?
   The old command reported all spaces and summarized the default space. The
   first slice should keep that behavior unless a caller-provided `space`
   filter is added.
3. Should admin metrics be executor-specific or engine-only? The first slice
   should prefer engine-only facts unless executor lifecycle timing is already
   tracked.
