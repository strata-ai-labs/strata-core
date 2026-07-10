# Runtime Resource Profile Architecture

Status: V1 architecture draft

## Purpose

Strata must run well on edge devices such as a Raspberry Pi Zero and on
server-class machines with the same binary. This is a core product promise, not
an implementation convenience.

The architecture must preserve automatic host-aware resource sizing while
removing the old coupling between hardware detection, product config mutation,
storage internals, and derived-state defaults.

The governing rule is:

```text
detect host -> choose product resource profile -> resolve runtime budgets
```

Storage, graph, vector, search, retrieval, and intelligence should receive
resolved budgets. They should not independently guess what kind of machine they
are running on.

## Related Documents

Read this with:

1. `docs/product/strata-v1-product-requirements.md`
2. `docs/product/strata-v1-non-functional-requirements.md`
3. `docs/architecture/strata-v1-architecture.md`
4. `docs/architecture/storage-architecture.md`
5. `docs/architecture/storage/l5-table-runtime.md`
6. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
7. `docs/architecture/storage/l9-storage-api-boundary.md`
8. `docs/architecture/storage/target-crate-shape-and-test-harness.md`

The current implementation evidence is in:

1. `crates/engine/src/database/profile.rs`
2. `crates/engine/src/database/config.rs`
3. `crates/storage/src/runtime_config.rs`
4. `crates/storage/src/pressure.rs`
5. `crates/storage/src/block_cache.rs`

## Product Requirement

A user should be able to install one Strata binary and use it across:

1. Raspberry Pi Zero and small edge devices.
2. Browser or sandbox cache targets.
3. Developer laptops.
4. Cloud VMs.
5. Large Xeon or EPYC servers.

The same product surface should work everywhere, but the resolved runtime
envelope should change:

1. Edge devices should use small caches, small write buffers, smaller table
   targets, limited background work, and conservative derived-state defaults.
2. Laptops should use balanced defaults.
3. Servers should use larger buffers, larger tables, more background workers,
   and higher-throughput maintenance defaults.
4. Explicit user configuration should override automatic defaults.
5. Constrained devices should fail with typed resource errors or graceful
   degradation before uncontrolled out-of-memory behavior.

## Current Behavior To Preserve

The current engine has a useful shape:

1. Hardware detection reads total RAM and available CPU cores.
2. Hosts are classified into `Embedded`, `Desktop`, and `Server`.
3. Embedded means less than 1 GiB RAM.
4. Desktop means 1 GiB through 16 GiB RAM.
5. Server means more than 16 GiB RAM.
6. Embedded profile reduces write buffer, block cache, background threads,
   table target size, L1 base size, compaction rate, and default vector dtype.
7. Server profile increases write buffer, background threads, table target
   size, and L1 base size.
8. User-supplied values are not clobbered.
9. A unified `memory_budget` overrides individual block-cache and write-buffer
   fields.
10. Cache-mode open applies the same profile so tiny hosts do not inherit
    oversized defaults.

The new architecture should keep these guarantees while replacing direct
mutation of public config with a resolved runtime plan.

## Non-Goals

This document does not define:

1. Final numeric defaults for every profile.
2. Benchmark targets.
3. A scheduler implementation.
4. A hosted fleet tuning service.
5. Per-primitive index algorithms.
6. Linux-only hardware detection.
7. A promise that every workload performs well on every device.

## Binding Decisions

1. **Runtime profiles are V1 substrate.**
   V1 is incomplete if the same binary cannot adapt from constrained edge
   devices to server-class hosts.

2. **Host probing belongs above storage.**
   Storage-next must not read `/proc/meminfo`, call `sysctl`, inspect CPU
   counts, classify the machine, or choose product defaults such as vector
   dtype.

3. **Engine-next owns product resource policy.**
   Engine-next should own host probing, profile classification, user override
   precedence, product-level budget allocation, and human-readable
   explanations.

4. **Storage-next owns storage-local budget spending.**
   Storage-next receives explicit resolved storage budgets and decides how they
   map to table cache, mutable tables, compaction, pressure, and maintenance
   mechanics.

5. **Resolved config is not persisted as user config.**
   Hardware-derived values are runtime facts. They should appear in diagnostics,
   but normal open should not write them back into `strata.toml` or durable
   database metadata as if the user chose them.

6. **Explicit config wins.**
   A user or administrator may pin memory budgets, worker counts, table sizes,
   cache sizes, index behavior, or derived-state limits. Automatic profiling
   must not override explicit choices.

7. **Resource ownership must be database-local by default.**
   The process-global block cache pattern should not return. Database-local
   caches and budgets make concurrent opens, tests, and embedded deployments
   predictable.

8. **Derived state may degrade before authored data.**
   On constrained devices, search indexes, vector indexes, graph analytics,
   auto-embedding queues, and intelligence features may be limited, deferred, or
   rebuilt. Authored committed data and storage recovery cannot depend on
   derived-state memory being available.

9. **The selected profile is observable.**
   `info`, `describe`, health, and diagnostic surfaces should expose the
   selected profile, relevant host facts, effective budgets, and whether each
   value was auto-derived or user-specified.

10. **Low-end failures are typed.**
    If a workload exceeds the configured envelope, Strata should return
    `resource_exhausted.memory`, `resource_exhausted.cache_capacity`,
    `resource_exhausted.disk`, or a more specific product error before the
    runtime becomes unstable.

## Responsibility Split

### Platform Probe

The platform probe gathers host facts:

1. Total memory, when available.
2. Available parallelism.
3. Platform family.
4. Process architecture, when useful.
5. Optional storage/backend latency class, if exposed by a backend later.
6. Optional sandbox/browser constraints, if explicitly provided by the host
   application.

The probe should be replaceable in tests. It should not be called from storage
internals.

### Resource Profile Classifier

The classifier maps host facts and explicit user profile selection to a product
profile.

V1 should retain the current broad classes:

1. `Embedded`
   Resource-constrained edge device.

2. `Desktop`
   Default developer or laptop class.

3. `Server`
   High-memory and high-core deployment.

4. `Custom`
   Explicit user or organization policy.

The classifier should be deterministic. If host facts are unavailable, it
should choose a conservative profile or require explicit configuration for
platforms where unsafe defaults would be risky.

### Resource Planner

The planner takes:

1. Host facts.
2. Selected profile.
3. User config.
4. Database mode.
5. Backend capability facts.
6. Optional organization policy.

It produces a `ResolvedRuntimePlan`.

The plan should include:

1. Storage runtime budget.
2. Engine derived-state budget.
3. Maintenance worker budget.
4. Graph/search/vector budget hints.
5. Intelligence/inference budget hints where configured.
6. Diagnostics explaining derivation sources.

### Storage Budget

The storage budget is storage-facing and product-agnostic.

It may include:

1. Table block-cache capacity.
2. Mutable-table write-buffer target.
3. Frozen mutable-table limit.
4. Table target size.
5. Level base size.
6. Table block size.
7. Bloom/filter budget.
8. Compaction I/O rate limit.
9. Maintenance memory pressure thresholds.
10. Maximum background storage tasks.

Storage may derive storage-local sub-budgets from a unified storage memory
budget, but it must not allocate product-wide memory among storage, graph,
vector, search, and intelligence by itself.

### Engine Derived-State Budget

Engine-next should own budgets for data capabilities and derived state:

1. Graph relationship indexes and analytics scratch memory.
2. Vector in-memory indexes and reload policy.
3. Search index caches and indexing batches.
4. Auto-embedding queues and shadow embedding work.
5. Retrieval recipe intermediate results.
6. Import/export buffering.
7. Branch diff and time-travel scan windows.

Engine may pass some of these budgets to intelligence-next or inference-next,
but storage must not know what the budgets mean.

## Profile Examples

The exact numbers should be benchmarked and refined, but the qualitative shape
is binding.

### Embedded

For Pi Zero, small IoT, and constrained edge devices:

1. Small table cache.
2. Small mutable-table target.
3. One or very few background workers.
4. Small table output targets.
5. Low compaction I/O rate.
6. Conservative graph/search/vector defaults.
7. Preference for compact derived-state representations.
8. Strict bounded scans and paginated product operations.
9. Early `resource_exhausted` errors for large imports, unbounded analytics, or
   oversized derived indexes.

### Desktop

For normal developer machines:

1. Balanced table cache and mutable table sizes.
2. Moderate background workers.
3. Default table targets.
4. Derived-state features enabled where configured.
5. Product operations remain bounded, but defaults favor usability over maximum
   throughput.

### Server

For high-memory, high-core deployments:

1. Larger mutable-table buffers.
2. Larger table targets.
3. More background workers.
4. Larger derived-state caches.
5. Higher compaction throughput.
6. More aggressive prefetching or indexing where proven safe.
7. Larger but still bounded query, diff, import, and retrieval windows.

## Configuration Model

The public configuration should distinguish:

1. Auto.
2. Explicit value.
3. Disabled, where disabling is meaningful.

The current `0 means auto` pattern should not spread into new architecture. It
may remain at compatibility edges, but internal runtime plans should use typed
states.

Recommended conceptual shape:

```text
UserRuntimeConfig
  resource_profile = auto | embedded | desktop | server | custom
  memory_budget = auto | bytes(N)
  storage_budget = auto | explicit(...)
  derived_state_budget = auto | explicit(...)

ResolvedRuntimePlan
  selected_profile
  host_facts_used
  storage_runtime_budget
  engine_runtime_budget
  derivation_sources
```

The planner should preserve source information:

1. User explicit.
2. Profile default.
3. Backend constraint.
4. Platform fallback.
5. Organization policy.

This source information is essential for diagnostics.

## Runtime Mode Interaction

### Durable Local

Durable local mode should use the selected profile for:

1. Storage table/cache sizing.
2. WAL and checkpoint scheduling policy.
3. Compaction rate and worker counts.
4. Derived-state rebuild and indexing budgets.

Durability guarantees do not change by profile. Only resource envelope and
performance behavior change.

### Cache / Browser

Cache mode is non-durable, but still needs a resource profile.

Browser and sandbox targets may not expose reliable RAM or CPU facts. They
should use conservative defaults unless the host application provides explicit
limits.

Cache mode must not use server-class in-memory defaults on unknown targets.

### Read-Only

Read-only mode may use smaller write-path budgets but still needs read cache,
derived-state, and scan budgets.

If recovery or repair would require write access, the failure remains a
read-only/lifecycle error, not a resource-profile decision.

### IPC

IPC clients must observe the resource plan of the owning database process. A
secondary IPC client should not attempt to resize storage resources directly.

Admin or diagnostic commands may request effective profile information through
engine-owned surfaces.

## Observability

Diagnostics should expose:

1. Selected resource profile.
2. Host facts used for classification.
3. Storage memory budget.
4. Table cache capacity.
5. Mutable-table target.
6. Frozen-table limit.
7. Table target size.
8. Compaction rate limit.
9. Background worker counts.
10. Derived-state budgets where enabled.
11. Values that were user-specified versus auto-derived.
12. Values reduced because of backend or platform constraints.
13. Current approximate memory use and pressure level.

These facts are useful for local debugging, Strata AI explanations, and future
fleet reports. Fleet reporting remains opt-in.

## Error And Degradation Policy

Resource pressure should produce bounded behavior.

Storage may report:

1. Memory pressure.
2. Cache capacity exhaustion.
3. Mutable-table pressure.
4. Table metadata pressure.
5. Compaction debt.
6. Backend capacity or quota failures.

Engine decides product behavior:

1. Retry after flush or compaction.
2. Return a typed resource error.
3. Defer derived-state work.
4. Disable optional derived-state acceleration.
5. Ask the user for a larger budget or a different operation shape.

Storage must never silently weaken committed-data durability because the host is
small. If durability cannot be maintained within the selected mode, open or
write must fail clearly.

## Testing Requirements

The architecture requires deterministic resource-profile tests.

Test families:

1. Fake platform probes for embedded, desktop, server, and unknown hosts.
2. Golden resolved-runtime-plan tests for default config under each profile.
3. Explicit-user-config tests proving automatic profiling does not clobber
   explicit values.
4. Unified memory-budget tests proving derived storage sub-budgets are bounded.
5. Cache/browser tests proving unknown hosts choose conservative defaults.
6. Storage tests proving database-local cache ownership and no process-global
   capacity races.
7. Low-memory stress tests proving oversized operations return typed resource
   errors or bounded pagination.
8. Server-profile tests proving larger budgets are applied without changing
   durable format or correctness.
9. IPC tests proving clients observe the owner process resource plan.
10. Diagnostic tests proving selected profile and effective budgets are visible
    and redacted where needed.

Performance benchmarks should run under at least desktop and server plans. Edge
tests should prioritize bounded behavior and absence of uncontrolled OOM over
throughput.

## Acceptance Criteria

The V1 architecture satisfies this requirement when:

1. A single binary can open cache and durable local databases on constrained
   devices and server-class machines.
2. Host detection and profile classification are engine-owned or product-owned,
   not storage-owned.
3. Storage receives resolved budgets and owns storage-local spending.
4. Product config can express auto versus explicit values without ambiguous
   internal sentinels.
5. Database-local caches replace hidden process-global resource ownership.
6. Resolved runtime plans are observable without being persisted as user
   choices.
7. Low-memory behavior is tested and returns typed failures or bounded
   degradation.
8. Derived-state memory pressure cannot compromise authored-data durability or
   recovery.

## Open Questions

These should be resolved during engine-next and storage-next implementation
planning:

1. What exact numeric defaults should embedded, desktop, and server profiles
   use after benchmarking?
2. Should profile names remain `Embedded`, `Desktop`, `Server`, or should
   product docs use friendlier names such as `edge`, `balanced`, and
   `throughput`?
3. What host facts are reliable on Windows, browser/WASM, mobile, containers,
   serverless runtimes, and agent sandboxes?
4. Should users be able to pin profile explicitly independent of detected
   hardware?
5. How should organization policy cap memory budgets on shared machines?
6. How should graph/search/vector derived-state budgets be divided under a
   unified product memory budget?
7. What minimum device class is required for V1 durable local support, and what
   device class is cache-only?
8. Which resource facts should future StrataHub Fleet report after opt-in?
