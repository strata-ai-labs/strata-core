# Runtime Memory Budget Test Plan

Status: revised test plan. Scope narrowed to **engine-next and storage-next**;
executor / CLI / SDK test families are deferred (listed at the end).

Implementation plan:
`docs/architecture/implementation-plans/runtime-memory-budget-implementation-plan.md`

Architecture anchor (authoritative):
`docs/architecture/runtime-resource-profile-architecture.md`

## Goal

Prove that Strata exposes, resolves, and **globally enforces** a memory budget
across storage-next and engine-next, for both cache and durable-local modes.

The central property under test: the budget is **global and cumulative**.
Strata-owned memory must stay under the cap over the life of the database — held
down by cumulative accounting plus eviction and flush — not merely by rejecting a
single oversized allocation. A test suite that only proves single-allocation
admission has not proven a budget.

The suite must fail if:

1. any production pool enforces per-allocation only (usage drops to zero after a
   call while the object is still live);
2. the global total is not the sum of live pool usage;
3. a process-global cache or capacity is introduced (ownership must be
   database-local);
4. cache mode silently uses an unlimited product budget;
5. a public product API can select the 64 KiB test profile;
6. storage-next probes host memory or classifies the machine;
7. engine open paths ignore explicit memory budgets;
8. auto/host-derived values are persisted as user-authored config;
9. low-memory operations grow without typed pressure or resource errors, or
   mutate visibly before failing;
10. diagnostics omit selected profile, effective budget, live usage, or sources.

## Test Layers

In scope:

1. Storage-next global-enforcement tests (the central family).
2. Storage-next API and lifecycle tests.
3. Engine-next runtime planner unit/golden tests.
4. Engine-next open/create integration tests.
5. Engine-next derived-state budget tests (vector/graph only for now).
6. Source/dependency guards.
7. Low-memory and server-profile smoke tests.

Deferred (see "Deferred Test Families"): executor, CLI, and SDK suites.

## Storage-Next Global Enforcement Tests

The load-bearing family. These prove the budget is real.

Location candidates:

1. `crates/storage-next/src/lifecycle/tests/budget.rs`
2. `crates/storage-next/src/lifecycle/tests/budget_runtime.rs`

Required tests:

1. `global_total_equals_sum_of_pool_usage`
   - the ledger's global total equals the sum of live per-pool usage at all
     times across a mixed workload.

2. `table_reader_pool_charges_cumulatively_not_per_allocation`
   - N concurrently live table readers each charge the pool;
   - tracked usage stays positive while they are live (regression guard against
     the `check_available`-forgets behavior);
   - usage returns to baseline only after the reservations drop.

3. `generated_artifact_and_manifest_pools_charge_cumulatively`
   - same cumulative-charge / release-on-drop contract for the
     `GeneratedArtifact` and `ManifestCatalog` pools.

4. `concurrent_reservations_that_collectively_exceed_pool_are_not_admitted`
   - several reservations that each individually fit but together exceed the pool
     budget do not all succeed silently; the excess evicts, defers, or fails.

5. `read_cache_evicts_to_stay_under_pool_budget`
   - inserting past the cache pool cap evicts;
   - live usage stays at or below the cap;
   - pinned entries are exempt from eviction but counted in usage.

6. `global_pressure_uses_summed_total_not_single_pool`
   - pressure level is computed from the summed global total, crossing
     warning/critical as the total crosses thresholds.

7. `memtable_flush_fires_on_global_pressure`
   - warning/critical on the global total triggers active/frozen rotation and
     flush through the existing maintenance loop.

8. `oversized_op_refuses_only_after_eviction_and_flush`
   - a typed `resource_exhausted.*` is returned only when eviction and flush
     cannot bring the total under the cap, and before any visible mutation.

9. `budget_is_database_local_not_process_global`
   - two databases opened with different budgets do not share capacity or
     interfere; no global capacity is mutated by one open.

Acceptance:

1. every memory-holding pool charges the ledger for the lifetime of the object;
2. the global total is accurate and drives pressure;
3. the cap is held by eviction/flush first and typed refusal last.

## Storage-Next API And Lifecycle Tests

Location candidates:

1. `crates/storage-next/src/api/tests/open_options.rs`
2. `crates/storage-next/src/api/tests/diagnostics.rs`
3. `crates/storage-next/tests/lifecycle_source_guard.rs`

Required tests:

1. `explicit_storage_budget_validates_all_pools`
   - exact byte values are accepted; mandatory pools reject zero; optional pools
     accept zero where documented; pool sums exceeding the total reject.

2. `explicit_storage_budget_flows_into_lifecycle_config`
   - the storage open plan carries the exact per-pool budget passed through API
     options.

3. `cache_mode_uses_resolved_budget_by_default`
   - a cache open with a small explicit budget reports that budget;
   - the product cache path does not call `StorageRuntimeBudget::unlimited()`.

4. `product_apis_cannot_select_the_64kib_test_profile`
   - no public budget policy or option resolves to the 64 KiB
     `low_memory_test_profile`; it is reachable only via a named test helper.

5. `cache_mode_low_budget_rejects_oversized_active_mutable_write`
   - rejection happens before visible mutation.

6. `durable_mode_low_budget_rejects_oversized_generated_artifact`
   - flush/compaction output exceeding the generated-artifact budget fails or
     defers before publication.

7. `budget_diagnostics_report_limits_live_usage_and_sources`
   - diagnostics include pool limits, **accurate live usage** (not zero while an
     object is live), pressure severity, and whether each value is exact or
     approximate.

8. `storage_source_guard_for_host_probing`
   - storage-next contains no `/proc/meminfo`, `sysctl`, host RAM, CPU
     classification, or profile-selection logic.

9. `storage_source_guard_for_unbounded_or_global_cache`
   - the unlimited helper is test-only and named; the product open path never
     selects it; no process-global cache/capacity static is present.

Acceptance:

1. explicit per-pool budget is visible at the storage API boundary;
2. product cache mode is budgeted; the test profile is unreachable;
3. storage errors name pool, requested amount, and limit where known.

## Engine Runtime Planner Tests

Location candidates:

1. `crates/engine-next/src/runtime/tests/profile.rs`
2. `crates/engine-next/src/runtime/tests/planner.rs`
3. `crates/engine-next/tests/runtime_profile_conformance.rs`

Required tests:

1. `fake_probe_selects_embedded_for_low_memory_host` (`< 1 GiB`).
2. `fake_probe_selects_desktop_for_mid_memory_host` (`1-16 GiB`).
3. `fake_probe_selects_server_for_high_memory_host` (`> 16 GiB`).
4. `unknown_host_uses_conservative_profile`
   - missing RAM facts do not select server/desktop defaults.
5. `explicit_profile_overrides_host_classification`.
6. `explicit_memory_budget_overrides_profile_total`
   - an exact total controls the storage and derived-state split; uncapped (a
     value above host RAM is honored, at most warned, never clamped).
7. `memory_budget_rejects_below_required_minimum`
   - the planner returns typed `invalid_config.memory_budget` instead of silently
     inflating.
8. `planner_emits_storage_pool_split_that_passes_storage_validation`
   - the derived per-pool storage budget sums within the storage total and sets
     mandatory pools non-zero (the engine→storage contract).
9. `allocation_split_matches_golden_per_profile`
   - storage/derived-state/scratch/reserve percentages match golden constants
     per profile.
10. `auto_plan_preserves_source_facts`
    - each value carries a `BudgetSource` (user / db-config / profile / host-probe
      / platform-fallback / backend-constraint / per-open).
11. `planner_does_not_persist_auto_derived_values`
    - creation intent stores `auto`, not concrete derived per-pool values.
12. `explicit_budget_intent_is_persistable`.
13. `cache_mode_and_durable_mode_share_profile_policy`
    - the two differ in durability mechanics, not in resource-profile ownership.
14. `no_sentinel_states_in_resolved_plan`
    - typed `Auto | Bytes(N)` throughout; no `0 means auto/unlimited`.

Acceptance:

1. the planner is pure and deterministic with fake probes;
2. source facts distinguish auto/profile/user/backend/platform;
3. exact per-profile budget numbers are golden-tested.

## Engine Open/Create Integration Tests

Location candidates:

1. `crates/engine-next/tests/runtime_budget_open.rs`
2. `crates/engine-next/tests/cache_behavior.rs`

Required tests:

1. `durable_create_with_auto_profile_opens_and_reports_plan`.
2. `durable_create_with_explicit_memory_budget_persists_intent`
   - close/reopen keeps the explicit cap.
3. `durable_create_with_auto_does_not_persist_host_derived_values`
   - fake host A creates with auto; fake host B reopens and receives a
     host-B-derived plan.
4. `durable_open_per_open_override_does_not_modify_database_config`.
5. `cache_open_with_memory_budget_reports_bounded_cache`.
6. `cache_open_without_explicit_budget_uses_conservative_on_unknown_host`
   - unknown host does not get server/unlimited budget.
7. `low_memory_durable_write_fails_before_commit_visibility`.
8. `low_memory_cache_write_fails_before_commit_visibility`.
9. `server_profile_increases_budget_without_format_change`
   - same durable format and correctness; only the envelope changes.
10. `engine_resolved_storage_budget_reaches_storage_unchanged`
    - the plan's storage budget is the budget storage actually enforces (no
      silent re-derivation below engine).

Acceptance:

1. durable and cache opens use the same resolved budget path;
2. auto-versus-explicit persistence semantics are proven;
3. resource failures are correctness-preserving.

## Derived-State Budget Tests

Only capabilities that exist today: `data/vector` and `data/graph`. Search,
import/export, auto-embedding, and inference budgets are deferred with their
subsystems.

Location candidates:

1. `crates/engine-next/tests/vector_budget_behavior.rs`
2. `crates/engine-next/tests/graph_budget_behavior.rs`

Required tests:

1. `vector_index_respects_derived_state_budget`
   - low budget degrades, defers, or errors without corrupting vector records.
2. `graph_analytics_respects_scratch_budget`
   - a large analytics operation returns a bounded error or a paginated/deferred
     result, never unbounded growth.
3. `derived_state_pressure_does_not_compromise_authored_data`
   - committed KV/JSON/event data and recovery are unaffected when derived-state
     budgets are exhausted.

Acceptance:

1. derived state cannot compromise authored data;
2. degraded features report clear capability/pressure facts;
3. operations remain bounded under the embedded profile.

## Source And Dependency Guards

1. storage-next imports no host-probing module and reads no `/proc/meminfo` /
   `sysctl`.
2. storage-next does not classify `embedded` / `desktop` / `server`.
3. storage-next introduces no process-global cache or capacity static.
4. engine-next owns runtime profile classification.
5. engine-next does not construct storage budgets that bypass its planner.
6. database-local config never stores host-derived or auto-derived values as
   user intent.

## Low-Memory Smoke Tests

Run under small deterministic budgets, not host-dependent memory limits, through
engine and storage open paths (no CLI dependency).

Required smoke:

1. cache create/open/diagnostics with a 16-64 MiB cap;
2. durable create/open/diagnostics with a 64-128 MiB cap;
3. small KV write/read/list;
4. small JSON set/get;
5. small vector upsert/query;
6. small graph node/edge create/list;
7. small event append/range;
8. oversized write returns a typed resource error before visibility;
9. sustained writes stay under the cap via eviction/flush (no unbounded growth);
10. close/reopen after budget pressure succeeds.

Acceptance:

1. no test relies on actual constrained hardware;
2. all failures are typed; no partial committed state after rejection;
3. steady-state memory stays bounded, proving global enforcement.

## Server Profile Smoke Tests

Run with a deterministic fake server profile and normal host resources.

Required smoke:

1. durable create/open under the server profile;
2. cache open under the server profile is still bounded, not unlimited;
3. 100K multi-primitive write/read smoke;
4. background maintenance enabled with a larger worker budget;
5. diagnostics report the server-selected profile and larger budget;
6. correctness matches desktop-profile behavior.

Acceptance:

1. the server profile changes the resource envelope only;
2. durable format and correctness do not change;
3. no benchmark-only bypass is introduced.

## Deferred Test Families

Captured for when downstream consumers are picked up; not implemented now.

1. **Executor** — open options accept memory budgets; `info`/JSON report profile,
   budget, sources, pressure; typed resource errors survive serialization;
   convenience constructors route through the planner.
2. **CLI** — `--profile` / `--memory-budget` on `new`/open/`--cache`; human and
   JSON budget reporting; byte-string parsing and validation; prompt-free
   noninteractive behavior.
3. **SDK** — Python/Node/Rust open parameters match engine semantics; structured
   resource errors; auto re-plans on host change; explicit budgets persist; no
   dependency on CLI-only config.

## Acceptance Criteria

The in-scope plan passes when:

1. storage and engine tests agree on global, cumulative budget semantics;
2. every memory-holding pool charges the ledger; usage is accurate, not
   zero-after-call;
3. the cap is held by eviction/flush, then typed refusal, before visible
   mutation;
4. cache mode is budgeted by default and the 64 KiB test profile is unreachable
   from product APIs;
5. ownership is database-local; no process-global cache/capacity exists;
6. auto and explicit persistence semantics are proven;
7. resource diagnostics expose profile, budgets, live usage, sources, and
   pressure;
8. the server profile raises limits without changing durable format or
   correctness;
9. source guards prevent regression to storage-owned host probing, hidden
   unlimited cache, or per-allocation-only enforcement.
