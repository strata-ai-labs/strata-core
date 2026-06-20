# Runtime Memory Budget Implementation Plan

Status: revised implementation plan. Scope narrowed to **engine-next and
storage-next**; downstream consumers (executor, CLI, SDK) are deferred and
captured only as the surface engine must expose.

Test plan:
`docs/architecture/implementation-plans/runtime-memory-budget-test-plan.md`

Architecture anchor (authoritative):
`docs/architecture/runtime-resource-profile-architecture.md`

## Objective

Restore the product-level memory budget and runtime profile contract for the
new architecture so the same binary runs on a constrained device (Raspberry Pi
Zero class) and on a server, with a memory cap that is actually enforced.

The single most important property: **the budget is global and cumulative.** A
user who sets a cap expects Strata-owned memory to stay under that cap over the
life of the database, not merely to reject one oversized allocation. The current
new-architecture storage budget enforces some pools per-allocation only (it
checks one request, then forgets it), which does not bound steady-state memory.
This plan makes the global cumulative budget the enforcement model and treats
per-allocation-only admission as a bug to remove, not a stage to keep.

Three concrete corrections drive this revision:

1. **Global, not per-allocation.** Every Strata-owned pool charges a single
   database-local budget ledger; the sum is the global total; eviction and flush
   keep the running total under the cap; pressure is computed against the total.
2. **Real profiles, no joke budgets.** The public constrained policy currently
   resolves to a 64 KiB test profile. Replace it with real `Embedded` /
   `Desktop` / `Server` profiles plus an uncapped explicit override.
3. **Cache mode is budgeted.** Cache mode currently maps to an unlimited budget.
   It must obey the same resolved budget as durable mode.

## Scope

In scope:

1. storage-next global budget enforcement, public resolved-budget API, cache-mode
   correction, real profiles, and budget diagnostics;
2. engine-next host probing, profile classification, resource planner, resolved
   runtime plan, open-option wiring, database-local intent persistence, and the
   engine to storage budget contract;
3. engine-next derived-state budgets for capabilities that already exist
   (`data/vector`, `data/graph`), with hooks left for capabilities not yet built.

Deferred (out of current scope, captured under "Downstream Consumers"):

1. executor-next open options and command output;
2. CLI flags and human-readable byte parsing;
3. SDK (Python/Node/Rust) open parameters and binding conformance.

These consume engine surfaces; engine must expose the plan and diagnostics in a
shape they can adopt later, but no executor/CLI/SDK code lands in this plan.

## Binding Decisions

These extend the architecture anchor's binding decisions; where they overlap,
the anchor wins.

1. **Memory budget is product-level user intent.**
   Users choose `auto`, a named profile, or an explicit byte cap. Storage
   receives only resolved storage budgets and never classifies the host.

2. **The budget is global and cumulative, enforced database-locally.**
   A single per-database ledger tracks live Strata-owned bytes across every
   pool. Admission, eviction, and flush all act on the running total. There is
   no process-global cache or capacity. Per-allocation-only checks are not an
   acceptable enforcement model for any pool that can hold memory after a call
   returns.

3. **The cap is honored by eviction and flush, then by typed refusal.**
   Evictable pools (table/block read cache) evict to stay under budget;
   non-evictable working sets (active/frozen memtables) flush under pressure;
   only when neither can bring the total under the cap does an operation fail
   with a typed `resource_exhausted.*` error, before any visible mutation.

4. **Explicit values persist; derived values do not.**
   Database-local config stores user intent (`profile = auto`,
   `memory_budget = 128MiB`). Host-derived and auto-derived per-pool values are
   runtime facts, observable in diagnostics, never written back as user choices.

5. **Cache mode is budgeted.**
   Cache mode is non-durable but obeys the selected memory envelope. An
   unlimited budget is reachable only through an explicitly named test helper
   that product open paths cannot select.

6. **The manual override is uncapped.**
   An explicit `memory_budget = bytes(N)` is honored for any `N`; the planner
   may warn when `N` exceeds host RAM but must not clamp it. "As much as the user
   wants" is a product promise.

7. **Typed states, no sentinels.**
   `Auto | Bytes(N)` and `Auto | Embedded | Desktop | Server | Custom` replace
   every `0 means auto / disabled / unlimited` sentinel in new code.

8. **Authored-data correctness outranks derived-state availability.**
   Under pressure, derived indexes, analytics, auto-embedding, and retrieval
   acceleration degrade or defer before storage durability or recovery weakens.

## Lessons From The Old Engine

The old architecture (`crates/engine/src/database/profile.rs`,
`crates/storage/src/{runtime_config,pressure,block_cache}.rs`) already solved the
hard parts of this. Its enforcement *discipline* is the model; some of its
*structure* is the anti-pattern we are correcting.

Adopt:

1. **Cumulative usage + eviction as the running bound.** The old block cache
   tracks an atomic `usage` and runs eviction "until usage is below capacity"
   before each insert. That running bound — not single-allocation admission — is
   what a budget means.
2. **A global pressure tracker.** `MemoryPressure` maps `current_total / budget`
   to `Normal | Warning | Critical` (0.7 / 0.9) and a scheduler flushes or
   compacts on it. Port this as a database-local total-vs-budget tracker.
3. **RAM-classified profiles in engine.** `Profile::classify` buckets hosts
   (`< 1 GiB` Embedded, `1-16 GiB` Desktop, `> 16 GiB` Server) and
   `apply_profile_if_defaults` sizes components only when the user has not
   overridden. Host probing already lived in engine, not storage — keep that.
4. **A unified budget that derives components.** `memory_budget`, when set,
   derives block-cache / write-buffer / immutable sizes; explicit per-component
   values apply only when the unified budget is absent. Precedence: explicit
   unified budget > explicit per-pool overrides > profile defaults > auto.
5. **Cache uses the same profile.** Old cache-mode opens applied the profile so
   small hosts did not inherit oversized in-memory defaults.

Discard:

1. **The process-global block cache.** `static GLOBAL_CACHE: OnceLock` +
   `set_global_capacity` is a process singleton shared across databases. The new
   ledger is database-local (architecture binding decision: the process-global
   pattern must not return).
2. **`0 means auto/disabled/unlimited` sentinels.** Pervasive in the old config
   (`memory_budget`, `block_cache_size`, pressure `budget`). Replace with typed
   states.
3. **Storage classifying the host.** The new storage layer never probes RAM/CPU
   or chooses a profile; it receives resolved bytes only.

## Current State (verified)

Verified against the live tree, not the prior draft's claims.

Storage-next:

1. `crates/storage-next/src/lifecycle/budget.rs`
   - Granular per-pool byte budgets exist; default total is **512 MiB** (block
     cache 64, table reader 128, active mutable 64, frozen mutable 128,
     maintenance queue 1, generated artifact 96, manifest catalog 16 MiB).
   - A **database-local** RAII ledger exists: `StorageBudgetLedger::reserve`
     returns a `StorageBudgetReservation` that charges bytes/count cumulatively
     and releases on `Drop`. This is the right primitive and is already
     database-local.
   - **Gap:** production paths for `TableReader`, `GeneratedArtifact`, and
     `ManifestCatalog` use `check_available` (single-allocation admission) and
     do **not** charge the ledger; after the call their tracked usage stays zero.
     The global total is therefore undercounted and steady-state memory is
     unbounded for those pools.
   - `low_memory_test_profile()` is **64 KiB total** and clearly test-oriented;
     a separate `scaled_closed_loop_test_profile()` is 4 MiB.
2. `crates/storage-next/src/api/options.rs`
   - Public `StorageBudgetPolicy` exposes only `Default` and `LowMemory`; there
     is no public explicit-byte budget. `StorageRuntimeBudget` (the per-pool
     type) is `pub(crate)`.
3. `crates/storage-next/src/api/runtime/open_close.rs`
   - **Foot-gun:** `map_budget_policy` maps the public `LowMemory` policy to the
     64 KiB `low_memory_test_profile()` — a test fixture on the product path.
   - **Cache unlimited:** `StorageMode::Cache => StorageRuntimeBudget::unlimited()`
     bypasses budget policy for product cache opens.
4. A pressure-driven maintenance loop already exists
   (`collect_storage_pressure_with_budget` plus the coverage scan in
   `lifecycle/durable/maintenance.rs`); it is blind to the three pools that do
   not charge the ledger.

Engine-next:

1. `crates/engine-next/src/runtime/` exists but is an empty stub
   (`mod.rs` is a single doc comment); `config/` is likewise a stub. No
   `HostFacts`, `HostProbe`, `ResourceProfile`, `MemoryBudget`,
   `ResolvedRuntimePlan`, or planner exist anywhere in engine-next.
2. `crates/engine-next/src/api/options.rs` open options expose only
   `default_branch`.
3. Capabilities present to budget later: `data/{kv,graph,json,vector,event}`.
   No search or auto-embedding module yet (auto-embedding is intelligence-next).

## Configuration Model

Typed, no sentinels:

```text
ResourceProfileSelection = Auto | Embedded | Desktop | Server | Custom
MemoryBudgetSelection    = Auto | Bytes(u64)

RuntimeResourceConfig
  profile: ResourceProfileSelection
  memory_budget: MemoryBudgetSelection
  storage_budget_overrides: optional explicit per-pool bytes
  derived_state_budget_overrides: optional explicit bytes
  background_worker_override: optional
```

Precedence (highest first), ported from the old engine and made typed:

1. explicit per-field overrides (storage/derived-state/worker);
2. explicit unified `memory_budget = Bytes(N)`;
3. selected or classified profile defaults;
4. host-classified `Auto`.

Database-local config stores only items 1-2 plus the profile *selection* and a
policy version. It never stores host-derived RAM facts, auto-derived per-pool
values, credentials, or transient pressure state.

## Resource Profiles

Engine-owned, RAM-classified (thresholds ported from the old engine):

1. `Embedded` — `< 1 GiB` host. Small caches and write buffers, one or zero
   background workers, bounded scans, early `resource_exhausted` for oversized
   imports/analytics.
2. `Desktop` — `1-16 GiB` host. Balanced defaults; the current storage 512 MiB
   default is the Desktop reference.
3. `Server` — `> 16 GiB` host. Larger buffers/targets, more workers, larger but
   still bounded derived-state caches; never unlimited.
4. `Custom` — explicit user/organization policy.

Starting reference totals (golden-tested constants, **re-baselined in
benchmarking** per Open Questions; per-pool split comes from the allocation
policy below):

| Profile  | Host RAM    | Starting total budget | Notes |
|----------|-------------|-----------------------|-------|
| Embedded | `< 1 GiB`   | ~128 MiB              | scale all pools down ~4x from Desktop |
| Desktop  | `1-16 GiB`  | 512 MiB (current)     | adopt current storage default as-is |
| Server   | `> 16 GiB`  | ~2 GiB                | larger buffers/targets, more workers |

When `memory_budget = Auto`, the planner derives the total from host facts
(starting heuristic: a bounded fraction of host RAM, e.g. RAM/4, clamped to the
profile band). When `memory_budget = Bytes(N)`, `N` is the total and overrides
the profile total (uncapped).

The 64 KiB `low_memory_test_profile` is removed from every product path. It may
remain as an explicitly named test-only constant that product APIs cannot
select.

## Resolved Runtime Plan

Conforms to the architecture anchor's shape, with source tracking:

```text
ResolvedRuntimePlan
  selected_profile
  host_facts_used
  storage_runtime_budget        // resolved per-pool bytes for storage-next
  engine_runtime_budget         // derived-state + maintenance + (later) inference
  derivation_sources            // per value: user | db-config | profile |
                                //            host-probe | platform-fallback |
                                //            backend-constraint | per-open
```

`engine_runtime_budget` carries the derived-state, maintenance-worker, and
(when present) inference sub-budgets the prior draft listed as separate top-level
fields; keeping them under one engine budget matches the anchor.

## Budget Allocation Policy

The **engine planner** owns division of the unified total into storage and
engine budgets. Storage may further divide its own slice into pools, but storage
never allocates the product-wide total across storage/derived-state/scratch.

Suggested first split (policy constants with golden tests, not scattered through
services):

1. storage runtime budget: 55-70 percent (higher share on smaller profiles);
2. engine derived-state budget: 15-30 percent;
3. import/export and command scratch: 5-10 percent;
4. intelligence/inference transient budget: optional, capped separately;
5. reserve / unallocated safety margin: at least 10 percent on constrained
   profiles.

The planner must emit a storage per-pool breakdown that satisfies storage's
boundary validation (pools sum within the storage total; mandatory pools
non-zero).

## Global Budget Enforcement Model

This is the central mechanism and the main correction over the prior draft.

1. **One database-local ledger.** Every Strata-owned pool charges the same
   per-database `StorageBudgetLedger`. The global total is the sum of pool
   usage. No process-global state.
2. **The total is runtime-summed; per-allocation admission stays.** The global
   total blends the ledger-charged pools with the database-wide runtime total
   (resident owned-table readers + memtables + block cache). The `TableReader`,
   `GeneratedArtifact`, and `ManifestCatalog` `check_available` calls remain
   single-allocation admission: Phase 1b found their allocations either become
   resident owned tables already counted by the runtime total (converting would
   double-count — flush checks the same bytes under both the artifact and reader
   pools) or are maintenance/relief buffers that must not be refused on the global
   total. RAII `reserve` stays reserved for future lazy-block-reader ranges.
3. **Eviction keeps evictable pools under cap.** The table/block read cache
   evicts (port the old CLOCK discipline, database-local) so it never exceeds
   its pool budget; pinned entries are exempt and counted.
4. **Flush keeps working sets under cap.** Active/frozen memtables rotate and
   flush under pressure; the existing maintenance/coverage loop already does
   this and only needs to read the now-accurate global total.
5. **Global pressure tracker.** A database-local tracker maps
   `live_total / budget` to `Normal | Warning | Critical` (thresholds ported
   from the old `MemoryPressure`) and drives flush/compaction/eviction.
6. **Typed refusal is last.** When eviction and flush cannot bring the total
   under the cap, the operation returns a typed `resource_exhausted.*` error
   before any visible mutation. Durability and recovery never weaken because the
   host is small.

## Storage-Next Work

1. **Keep per-allocation admission for the transient pools (not a conversion).**
   `TableReader`, `GeneratedArtifact`, and `ManifestCatalog` stay on
   `check_available`. Their cumulative cost is captured by the runtime total
   (resident readers + memtables + block cache), so the RAII conversion was found
   redundant (it double-counts resident outputs) and dropped in Phase 1b. The
   load-bearing change is the runtime total + admission, not per-pool charging.

2. **Database-local eviction for evictable pools** — satisfied by the existing
   block-cache LRU, which evicts synchronously on the hot path to stay under its
   pool budget; its usage is a true running value, not zero-after-call. No
   additional work.

3. **Defer optional maintenance under global memory pressure.** *(Corrected from
   "fire flush/compaction on `Warning`/`Critical`", which does not hold under V1's
   whole-object materialized readers: flush converts memtable bytes into resident
   owned-table bytes — ~zero net relief — and a compaction holds its inputs and
   output resident at once, a transient spike. Only block-cache eviction and
   admission refusal bound memory.)* Thread the live `global_pressure()` into the
   two durable maintenance scheduling sites
   (`schedule_post_commit_maintenance_for_branch` and
   `schedule_maintenance_coverage_after_branch`) via
   `LifecycleStoragePressure::deferred_under_global_memory_pressure`: at the
   high-water mark, hold back optional (`Background`) flush/compaction while
   required (`Urgent`/`BlockMutatingAdmission`) maintenance — which gates write
   admission — still runs. Eviction stays synchronous (block-cache LRU).
   *Limitation:* gating is at enqueue time, so a task queued before pressure rose
   can still execute under pressure (small window, bounded by the maintenance
   queue); execute-time gating is a possible future refinement.

4. **Promote a public explicit storage budget type.** Add a public resolved
   per-pool byte budget under `crates/storage-next/src/api/` (e.g.
   `StorageMemoryBudget`) without exposing lifecycle internals. Support exact
   byte values for block cache, table readers, active mutable, frozen mutable,
   maintenance queue, generated artifacts, and manifest catalog. Validate at the
   boundary: pools sum within total; mandatory pools non-zero.

5. **Fix the policy mapping.** Stop mapping the public `LowMemory` policy to the
   64 KiB test profile. Keep `Default` for convenience; either remove `LowMemory`
   from the product enum in favor of explicit bytes or repoint it at a real
   profile. The test profile becomes an explicitly named test-only constant.

6. **Make cache mode obey budgets.** Remove
   `StorageMode::Cache => StorageRuntimeBudget::unlimited()` from product opens;
   cache uses the resolved budget like durable. Keep an explicitly named
   test/unlimited helper unreachable from product APIs.

7. **Expose budget diagnostics.** Selected storage budget; per-pool limit and
   live usage (now accurate); pressure severity by pool and overall; budget
   rejection facts; whether cache mode is bounded or explicitly unlimited;
   whether each usage value is exact or approximate (`DiagnosticsBudgetAccuracy`:
   `Tracked` for the runtime-summed and counted pools, `AdmissionOnly` for the
   per-allocation `check_available` pools; the database-wide total is `Tracked`).

8. **Source guard.** Storage-next contains no host probing, RAM/CPU inspection,
   or profile classification.

## Engine-Next Work

1. **Build the runtime module.** Under `crates/engine-next/src/runtime/`:
   `HostFacts`, `HostProbe` (trait + real platform probe + fake), `ResourceProfile`,
   `MemoryBudget`, `RuntimeResourceConfig`, `ResolvedRuntimePlan`, `BudgetSource`.

2. **Deterministic host probing behind a trait.** Real probes for known
   platforms; conservative defaults for unknown hosts unless the host supplies an
   explicit budget; fakes for tests. Never called from storage.

3. **Pure planner.** Input: host facts, open mode, user config, database-local
   config, backend capabilities, per-open overrides. Output: `ResolvedRuntimePlan`.
   Pure and unit/golden-testable; owns the classification and the allocation
   split; mirrors the old `classify` + `apply_profile_if_defaults` precedence.

4. **Extend open options.** `CacheOpenOptions` and `DurableLocalOpenOptions`
   accept profile and memory-budget selections. Durable open reads database-local
   resource intent and merges per-open overrides; create writes intent only for
   explicit user values.

5. **Translate to storage.** Map `ResolvedRuntimePlan.storage_runtime_budget` to
   the new public storage budget type and pass it at open. Storage never selects
   a product profile.

6. **Apply derived-state budgets where capabilities exist.** Start with
   `data/vector` and `data/graph` (in-memory index / analytics scratch); leave
   typed hooks for search, import/export, auto-embedding, and retrieval as those
   land. Start with hard bounds and clear deferred/unsupported behavior where
   exact enforcement is not ready.

7. **Expose diagnostics for later consumers.** `DatabaseOpenSummary` / `info` /
   `health` surface selected profile, host facts used, effective budgets,
   per-value source, and current pressure — in a shape executor/CLI/SDK can
   adopt without re-deriving anything.

## Downstream Consumers (Deferred)

Not implemented in this plan; recorded so the engine surface is consumable later:

1. **Executor** — open options carrying profile/budget; info/diagnostic output
   for profile, effective budget, sources, and pressure; convenience
   constructors that route through the planner.
2. **CLI** — `--profile` / `--memory-budget` on `new`/open/`--cache`; human and
   JSON budget reporting; human-readable byte parsing; prompt-free noninteractive
   behavior.
3. **SDK** — `memory_budget` / `profile` on open; typed resource errors; no
   dependency on CLI-only config.

When these are picked up they consume the engine `ResolvedRuntimePlan` and
diagnostics; no new semantics should be introduced below engine.

## Error Model

Typed errors (reuse where they exist):

1. `resource_exhausted.memory`
2. `resource_exhausted.storage_budget`
3. `resource_exhausted.cache_capacity`
4. `resource_exhausted.mutable_table`
5. `resource_exhausted.generated_artifact`
6. `resource_exhausted.derived_state`
7. `invalid_config.memory_budget`
8. `unsupported_capability.unbounded_cache`

Each must include, where known: requested bytes/count; limit; pool or feature;
selected profile; and a remediation hint. Remediation never instructs normal
users to run low-level maintenance.

## Implementation Order

Engine + storage only; storage first because it is the contract the planner
targets.

1. **Storage global enforcement.** Add the runtime-summed database-wide total and
   wire it into commit admission (refuse over-budget before mutation). The block
   cache already evicts via its own LRU; the per-allocation pool checks stay as
   admission. (Makes the budget real before exposing it.)
2. **Storage public budget API.** Public resolved-byte type, boundary
   validation, and diagnostics. Keep lifecycle internals private.
3. **Storage cache + policy correction.** Cache obeys resolved budget; remove the
   64 KiB product mapping; relegate unlimited and the test profile to named
   test-only helpers.
4. **Engine runtime planner.** Host facts, probe trait + fakes, profiles, memory
   budget, resolved plan, sources. Golden planner tests before any wiring.
5. **Engine open wiring.** Extend open options; read/write database-local intent;
   translate the resolved storage budget into the new storage API.
6. **Engine diagnostics.** Expose profile, sources, budgets, and pressure in
   engine open summary / info / health.
7. **Engine derived-state budgets.** Wire `data/vector` and `data/graph`
   consumers to the resolved engine budget; hard bounds plus deferred behavior
   for not-yet-built capabilities.
8. **Low-end and server verification.** Deterministic low-memory tests; cache and
   durable smoke under small budgets; server-profile checks that correctness and
   durable format are unchanged.

## Acceptance Criteria

1. Strata-owned memory stays bounded by the configured cap over time, enforced by
   cumulative accounting plus eviction/flush — not single-allocation admission.
2. Engine and storage open paths accept explicit memory budgets and named
   profiles; cache and durable share the resolved budget path.
3. Cache mode no longer defaults to unlimited; the 64 KiB test profile is
   unreachable from product APIs.
4. Engine owns host probing and profile classification; storage receives resolved
   budgets and never probes the host.
5. Low-memory oversized operations return typed resource errors or bounded
   degradation, before any visible mutation, without weakening durability.
6. Diagnostics expose selected profile, effective budgets, per-value sources, and
   live pressure; usage values are accurate (charged), not zero-after-call.
7. Explicit user budgets persist as intent; auto/host-derived values do not.
8. Resource ownership is database-local; no process-global cache or capacity is
   introduced.
9. The same binary configures for constrained edge and server-class operation
   without code changes.

## Out Of Scope

1. OS-enforced process RSS limits or cgroup management.
2. Perfect accounting for allocator overhead and third-party native allocations.
3. Native local-model memory enforcement inside llama.cpp beyond model/runtime
   diagnostics and admission checks.
4. Distributed/fleet policy management and hosted StrataHub fleet reporting.
5. Rewriting storage table formats or changing durability semantics by profile.
6. Executor, CLI, and SDK implementation (see Downstream Consumers).

## Open Questions

1. Exact per-profile per-pool default numbers after benchmarking (the table
   above is a starting point).
2. Internal profile names are `Embedded` / `Desktop` / `Server`; friendlier
   external names (`edge` / `balanced` / `throughput`) are a product-doc decision,
   out of engine+storage scope.
3. Auto-budget derivation heuristic from host RAM (RAM/4 clamped to band is a
   starting point); refine with benchmarking.
4. Whether `Auto` re-plans on every durable open (recommended) versus pinning at
   creation. Recommended answer: store intent, re-plan `Auto` on open, persist
   explicit pins only.
5. How the engine derived-state slice subdivides among vector/graph (and later
   search/import/auto-embedding) under a unified total.
6. Eviction policy detail for the new database-local read cache (CLOCK vs LRU)
   and how pinned entries are accounted.
