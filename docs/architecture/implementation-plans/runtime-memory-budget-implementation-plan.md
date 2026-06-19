# Runtime Memory Budget Implementation Plan

Status: draft implementation plan

Test plan:
`docs/architecture/implementation-plans/runtime-memory-budget-test-plan.md`

Architecture anchor:
`docs/architecture/runtime-resource-profile-architecture.md`

Product anchor:
`docs/product/strata-v1-cli-sdk-experience.md`

## Objective

Restore the product-level memory budget and runtime profile contract for the
new architecture.

Strata must be able to run the same binary on constrained devices such as a
Raspberry Pi Zero and on server-class machines. Users and SDK hosts must be
able to cap Strata-owned memory so small devices fail with typed resource
errors or bounded degradation instead of uncontrolled out-of-memory behavior.

The current storage-next implementation has useful storage-local budget
mechanics, but the product-level control is incomplete:

1. storage-next has partial `StorageRuntimeBudget` enforcement;
2. storage-next only exposes `Default` and `LowMemory` budget policies;
3. cache mode currently maps to an effectively unlimited budget;
4. engine-next open options do not expose profile or memory cap;
5. executor-next opens cache and durable databases with default options;
6. CLI and SDK experience has no stable way to request or inspect a budget;
7. diagnostics do not expose a resolved runtime plan.

This plan closes that gap without adding a daemon, hidden global state, or
storage-owned host probing.

## Binding Decisions

1. **Memory budget is product-level user intent.**
   Users may choose `auto`, a named profile, or an explicit byte cap. Storage
   receives only resolved storage budgets.

2. **Host probing belongs above storage.**
   Storage-next must not inspect host RAM, CPU, platform, or container facts.

3. **Resolved values are runtime facts.**
   Host-derived values are observable diagnostics. They are not persisted as if
   the user explicitly chose them.

4. **Explicit values persist.**
   Database-local config may store user intent such as `profile = "auto"` or
   `memory_budget = "128MiB"`. It should not store an auto-derived 128 MiB
   storage cache as a user-authored setting.

5. **Cache mode is still budgeted.**
   Cache mode is non-durable, but it must obey the selected memory envelope.
   Cache is not allowed to become a pathless unlimited-memory mode by default.

6. **The budget is a Strata-owned memory budget, not an OS RSS guarantee.**
   It bounds Strata-owned caches, mutable tables, table readers, generated
   artifacts, maintenance queues, derived-state indexes, import/export buffers,
   and inference/intelligence working sets where those layers participate.
   Allocator overhead, runtime stack memory, third-party native allocations,
   and process-level RSS are reported as caveats unless separately managed.

7. **Authored data correctness outranks derived-state availability.**
   Under pressure, derived indexes, analytics, auto-embedding, and retrieval
   acceleration may degrade or defer before storage durability or recovery
   semantics weaken.

## Source Evidence

Old architecture evidence:

1. `crates/engine/src/database/profile.rs`
2. `crates/engine/src/database/config.rs`
3. `crates/storage/src/runtime_config.rs`
4. `crates/storage/src/pressure.rs`
5. `crates/storage/src/block_cache.rs`

Current new-architecture evidence:

1. `crates/storage-next/src/lifecycle/budget.rs`
   - storage-local pool budgets and partial enforcement exist;
   - default storage budget is 512 MiB;
   - low-memory profile is fixed and test-oriented;
   - cache mode can use an unlimited budget.
2. `crates/storage-next/src/api/options.rs`
   - public storage API exposes `StorageBudgetPolicy::{Default, LowMemory}`;
   - explicit byte budget is not public.
3. `crates/storage-next/src/api/runtime/open_close.rs`
   - durable modes map budget policy to storage budget;
   - cache mode bypasses budget policy and uses unlimited storage budget.
4. `crates/engine-next/src/api/options.rs`
   - cache and durable open options only expose default branch.
5. `crates/executor-next/src/executor.rs`
   - executor opens cache and durable with default engine options.
6. `docs/architecture/runtime-resource-profile-architecture.md`
   - already defines the intended `UserRuntimeConfig` and
     `ResolvedRuntimePlan` shape.

## Public Product Shape

CLI examples:

```sh
strata new ./edge-db --profile embedded
strata new ./edge-db --memory-budget 128MiB
strata ./edge-db --memory-budget 128MiB
strata --cache --memory-budget 64MiB
strata --db ./edge-db info
```

SDK examples:

```python
db = strata.open("./edge-db", create=True, memory_budget="128MiB")
db = strata.open_cache(memory_budget="64MiB")
```

```ts
const db = await open("./edge-db", {
  create: true,
  memoryBudget: "128MiB",
});
const scratch = await openCache({ memoryBudget: "64MiB" });
```

Rust conceptual shape:

```rust
let db = strata::OpenOptions::new()
    .create(true)
    .memory_budget(MemoryBudget::bytes(128 * 1024 * 1024))
    .open("./edge-db")?;

let cache = strata::OpenOptions::cache()
    .memory_budget(MemoryBudget::bytes(64 * 1024 * 1024))
    .open()?;
```

## Configuration Model

Introduce explicit typed states:

```text
ResourceProfileSelection
  Auto
  Embedded
  Desktop
  Server
  Custom

MemoryBudgetSelection
  Auto
  Bytes(u64)

RuntimeResourceConfig
  profile
  memory_budget
  storage_budget_overrides
  derived_state_budget_overrides
  background_worker_override
```

Avoid sentinel states such as `0 means auto`.

Database-local config stores:

1. user-selected profile, if any;
2. user-selected memory budget, if any;
3. explicit storage or derived-state overrides, if any;
4. profile version or policy version for diagnostics/migrations.

Database-local config does not store:

1. host-derived RAM facts;
2. auto-derived per-pool storage budget values as user choices;
3. cloud credentials;
4. transient pressure state.

## Resolved Runtime Plan

Introduce a runtime plan at the engine boundary:

```text
ResolvedRuntimePlan
  selected_profile
  profile_source
  memory_budget
  memory_budget_source
  host_facts_used
  storage_runtime_budget
  engine_derived_state_budget
  maintenance_budget
  inference_budget
  diagnostics
```

Source values should distinguish:

1. user explicit;
2. database-local config;
3. profile default;
4. host probe;
5. platform fallback;
6. backend constraint;
7. SDK/CLI override for this open.

## Budget Allocation Policy

The planner owns division of the product memory budget.

Suggested first allocation:

1. storage runtime budget: 55-70 percent depending on profile;
2. engine derived-state budget: 15-30 percent;
3. import/export and command scratch budget: 5-10 percent;
4. intelligence/inference transient budget: optional and capped separately;
5. reserve/unallocated safety margin: at least 10 percent for constrained
   profiles.

Exact percentages should be policy constants with golden tests, not scattered
through engine services.

For `Embedded`:

1. block cache may be zero or small;
2. active mutable target is small;
3. frozen table count is small;
4. background workers are one or disabled inline depending on platform;
5. derived-state indexes default to bounded or disabled acceleration;
6. operations that require large working sets must paginate, defer, or return
   typed resource errors.

For `Desktop`:

1. balanced defaults;
2. moderate table cache and active mutable size;
3. background maintenance enabled;
4. derived-state features enabled when configured.

For `Server`:

1. larger mutable buffers and table targets;
2. more background workers;
3. larger derived-state caches;
4. still bounded, never unlimited.

## Storage-Next Work

1. Promote an explicit storage budget API shape.
   - Add a public storage-facing budget spec or resolved budget type under
     `crates/storage-next/src/api/`.
   - Do not expose lifecycle internals directly if they contain storage-private
     pools or unstable mechanics.
   - Support exact byte values for block cache, table readers, active mutable,
     frozen mutable, maintenance queue, generated artifacts, and manifest
     catalog.

2. Replace `StorageBudgetPolicy::LowMemory` as the only constrained public
   option.
   - Keep `Default` and `LowMemory` for compatibility/testing if useful.
   - Add explicit resolved budget input from engine.
   - Validate exact values at storage API boundaries.

3. Make cache mode obey budgets.
   - Remove the unconditional `StorageMode::Cache => StorageRuntimeBudget::unlimited()`
     default for product opens.
   - Cache mode may still have an explicit unlimited test/helper path if it is
     named as such and cannot be selected accidentally by product APIs.
   - Default cache opens should use the resolved profile budget.

4. Strengthen enforcement caveats.
   - Keep existing pool admission checks.
   - Identify pools that still only check per-allocation size.
   - Add follow-up work for cumulative RAII reservations where needed.
   - Make diagnostics honest about approximate versus reserved usage.

5. Expose budget diagnostics.
   - Selected storage budget.
   - Current approximate usage by pool.
   - Pressure severity by pool.
   - Budget rejection facts.
   - Whether cache mode was bounded or explicitly unlimited.

## Engine-Next Work

1. Add runtime resource modules.
   - Suggested location: `crates/engine-next/src/runtime/`.
   - Types: `HostFacts`, `HostProbe`, `ResourceProfile`, `MemoryBudget`,
     `RuntimeResourceConfig`, `ResolvedRuntimePlan`, `BudgetSource`.

2. Add deterministic host probing.
   - Use platform APIs behind a replaceable trait.
   - Never call probes from storage.
   - Provide fake probes for tests.
   - For unknown hosts, choose conservative defaults unless the host supplies
     an explicit budget.

3. Add planner.
   - Input: host facts, open mode, user config, database-local config, backend
     capabilities, and per-open overrides.
   - Output: `ResolvedRuntimePlan`.
   - The planner must be pure and unit-testable.

4. Extend open options.
   - `CacheOpenOptions` and `DurableLocalOpenOptions` should accept resource
     profile and memory budget selections.
   - Durable open should read database-local resource intent and merge it with
     per-open overrides.
   - Create should write database-local resource intent when the user selected
     explicit values.

5. Pass storage budgets to storage-next.
   - Translate `ResolvedRuntimePlan.storage_runtime_budget` to storage API
     options.
   - Do not let storage select product profiles.

6. Apply engine derived-state budgets.
   - Vector indexes.
   - Graph indexes and analytics scratch.
   - Search indexes and rebuild batches.
   - Branch diff/history scan windows.
   - Import/export buffers.
   - Auto-embedding queues.

7. Expose diagnostics.
   - `DatabaseOpenSummary`.
   - `info`.
   - `describe`.
   - `health`.
   - executor output structures.

## Executor, CLI, And SDK Work

1. Executor open options.
   - Add `ExecutorOpenOptions` or equivalent for cache/durable opens.
   - Support profile and memory budget.
   - Preserve default convenience constructors.

2. Command contract.
   - Add info/diagnostic output fields for selected profile, effective budget,
     budget sources, and current pressure.
   - Avoid stringly config mutation for low-level storage internals.

3. CLI.
   - `strata init` reports detected profile and local AI setup.
   - `strata new` accepts `--profile` and `--memory-budget`.
   - `strata <path>` and `strata --db <path>` accept per-open overrides.
   - `strata --cache` accepts `--profile` and `--memory-budget`.
   - `strata info` reports effective budget and source.

4. SDK.
   - Python/Node/Rust open APIs accept profile and memory budget.
   - SDK errors expose typed resource exhaustion.
   - SDKs do not require `strata init`.

5. Local AI.
   - Local model assets still live under `~/.strata`.
   - Local inference memory should have its own explicit or derived budget
     facts.
   - The planner may warn when selected local models exceed the configured
     runtime envelope.

## Error Model

Add or reuse typed errors:

1. `resource_exhausted.memory`
2. `resource_exhausted.storage_budget`
3. `resource_exhausted.cache_capacity`
4. `resource_exhausted.mutable_table`
5. `resource_exhausted.generated_artifact`
6. `resource_exhausted.derived_state`
7. `invalid_config.memory_budget`
8. `unsupported_capability.unbounded_cache`

Errors must include:

1. requested bytes or count where known;
2. limit;
3. pool or feature;
4. selected profile;
5. remediation hint where possible.

## Implementation Order

1. **Document reconciliation**
   - Align product docs with the runtime architecture: persist user intent,
     expose resolved facts, and do not persist auto-derived values as user
     choices.

2. **Storage explicit budget API**
   - Add public storage budget input and diagnostics.
   - Keep existing lifecycle budget internals private.
   - Add validation for exact pool values.

3. **Cache mode budget correction**
   - Make product cache opens use resolved budgets.
   - Keep only explicitly named test/unlimited helpers where needed.

4. **Engine runtime planner**
   - Add host facts, profile selection, memory budget selection, and resolved
     runtime plan types.
   - Add deterministic planner tests before wiring open.

5. **Engine open wiring**
   - Extend cache/durable open options.
   - Read/write database-local resource intent.
   - Pass resolved storage budget into storage-next.

6. **Diagnostics**
   - Expose profile, source facts, budgets, and pressure in engine and executor
     info/health output.

7. **Executor and CLI/SDK surface**
   - Add open options and flags.
   - Add parsing and validation for human-readable byte strings.
   - Keep noninteractive commands prompt-free.

8. **Derived-state budget integration**
   - Wire vector/search/graph/import/export/inference consumers to the resolved
     engine budget.
   - Start with hard bounds and clear unsupported/deferred behavior where
     exact enforcement is not ready.

9. **Low-end smoke and server-scale verification**
   - Run deterministic low-memory tests.
   - Run cache and durable smoke with small budgets.
   - Run normal benchmark profiles to ensure server defaults still scale.

## Acceptance Criteria

This work is complete when:

1. users can open cache and durable databases with explicit memory budgets;
2. SDKs and CLI expose the same profile/budget semantics;
3. cache mode no longer defaults to an unlimited product budget;
4. engine owns host probing and profile classification;
5. storage receives resolved storage budgets and never probes host memory;
6. low-memory oversized operations return typed resource errors or bounded
   degradation;
7. diagnostics expose selected profile, effective budgets, sources, and
   pressure facts;
8. explicit user budget values are persisted as intent where appropriate;
9. auto-derived values are not persisted as user-authored config;
10. the same binary can be configured for constrained edge and server-class
    operation without code changes.

## Out Of Scope

1. OS-enforced process RSS limits or cgroup management.
2. Perfect accounting for allocator overhead.
3. Native local model memory enforcement inside llama.cpp beyond explicit
   model/runtime diagnostics and admission checks.
4. Distributed/fleet policy management.
5. Hosted StrataHub fleet reporting.
6. Rewriting storage table formats.
7. Changing durability semantics by profile.

## Open Questions

1. Exact profile thresholds and default budget numbers after benchmarking.
2. Whether CLI profile names should be `embedded/desktop/server` or
   `edge/balanced/throughput`.
3. Whether default auto profile should adapt on every open or be pinned at
   database creation for durable databases. The recommended answer is: store
   user intent, adapt auto on open, and persist explicit pins only.
4. How to expose memory-budget diagnostics in JSON output without overwhelming
   normal users.
5. Whether local inference model loading should reserve from the same product
   memory budget or a separate explicit inference budget.
