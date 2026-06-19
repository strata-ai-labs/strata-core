# Runtime Memory Budget Test Plan

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/runtime-memory-budget-implementation-plan.md`

Architecture anchor:
`docs/architecture/runtime-resource-profile-architecture.md`

## Goal

Prove that Strata exposes, resolves, enforces, and reports memory budgets
consistently across storage-next, engine-next, executor-next, CLI, SDK-style
open paths, cache mode, and durable-local mode.

The suite must fail if:

1. storage-next probes host memory;
2. cache mode silently uses an unlimited product budget;
3. engine/executor open paths ignore explicit memory budgets;
4. auto profile values are persisted as user-authored config;
5. low-memory operations grow without typed pressure or resource errors;
6. diagnostics omit the selected profile, effective budget, or source facts;
7. derived-state features can exceed their resolved budget without bounded
   behavior;
8. SDK and CLI semantics diverge.

## Test Layers

1. Storage-next API and lifecycle tests.
2. Engine-next runtime planner unit tests.
3. Engine-next open/create integration tests.
4. Executor-next command and diagnostics tests.
5. CLI contract tests or command harness tests.
6. SDK-style examples or binding conformance tests.
7. Source/dependency guards.
8. Low-memory and server-profile smoke benchmarks.

## Storage-Next Tests

Location candidates:

1. `crates/storage-next/src/api/tests/open_options.rs`
2. `crates/storage-next/src/api/tests/diagnostics.rs`
3. `crates/storage-next/src/lifecycle/tests/budget.rs`
4. `crates/storage-next/src/lifecycle/tests/budget_runtime.rs`
5. `crates/storage-next/tests/lifecycle_source_guard.rs`

Required tests:

1. `explicit_storage_budget_validates_all_pools`
   - exact byte values are accepted;
   - mandatory pools reject zero;
   - optional pools accept zero where documented;
   - overflowing pool sums reject.

2. `explicit_storage_budget_flows_into_lifecycle_config`
   - storage open plan carries the exact budget passed through API options.

3. `cache_mode_uses_resolved_budget_by_default`
   - cache open with a small explicit budget reports that budget;
   - cache open does not call `StorageRuntimeBudget::unlimited()` unless an
     explicitly named test/unlimited helper is used.

4. `cache_mode_low_budget_rejects_oversized_active_mutable_write`
   - budget rejection happens before visible mutation.

5. `durable_mode_low_budget_rejects_oversized_generated_artifact`
   - flush/compaction output exceeding generated-artifact budget fails or
     defers before publication.

6. `budget_diagnostics_report_limits_usage_and_sources`
   - diagnostics include pool limits, approximate usage, pressure severity, and
     whether usage is exact or approximate.

7. `storage_source_guard_for_host_probing`
   - storage-next must not contain direct `/proc/meminfo`, `sysctl`, host RAM,
     CPU classification, or profile-selection logic.

8. `storage_source_guard_for_unbounded_product_cache`
   - unlimited cache helper is test-only or explicitly named;
   - product open path does not silently choose unlimited.

Acceptance:

1. explicit budget is visible at the storage API boundary;
2. product cache mode is budgeted;
3. all storage errors name pool, requested amount, and limit where known.

## Engine Runtime Planner Tests

Location candidates:

1. `crates/engine-next/src/runtime/tests/profile.rs`
2. `crates/engine-next/src/runtime/tests/planner.rs`
3. `crates/engine-next/tests/runtime_profile_conformance.rs`

Required tests:

1. `fake_probe_selects_embedded_for_low_memory_host`
   - host under the embedded threshold selects embedded profile.

2. `fake_probe_selects_desktop_for_mid_memory_host`
   - mid-memory host selects desktop profile.

3. `fake_probe_selects_server_for_high_memory_host`
   - high-memory/high-core host selects server profile.

4. `unknown_host_uses_conservative_profile`
   - missing RAM facts do not select server defaults.

5. `explicit_profile_overrides_host_classification`
   - user-selected profile wins over fake host facts.

6. `explicit_memory_budget_overrides_profile_default`
   - exact memory cap controls storage and derived-state allocation.

7. `memory_budget_rejects_too_small_required_minimum`
   - planner returns typed invalid config instead of silently inflating.

8. `auto_plan_preserves_source_facts`
   - each budget value has source information.

9. `planner_does_not_persist_auto_derived_values`
   - creation config stores `auto` intent, not concrete derived per-pool values.

10. `explicit_budget_intent_is_persistable`
    - explicit memory cap can be written and read as user intent.

11. `cache_mode_and_durable_mode_share_profile_policy`
    - cache and durable differ in durability mechanics but not in resource
      profile ownership.

12. `read_only_mode_reduces_write_path_budgets_without_losing_read_budget`
    - read-only profile still has read cache/scan budget facts.

Acceptance:

1. planner is pure and deterministic with fake probes;
2. source facts distinguish auto/profile/user/backend/platform;
3. exact budget numbers are golden-tested per profile.

## Engine Open/Create Integration Tests

Location candidates:

1. `crates/engine-next/tests/open_behavior.rs`
2. `crates/engine-next/tests/runtime_budget_open.rs`
3. `crates/engine-next/tests/cache_behavior.rs`

Required tests:

1. `durable_create_with_auto_profile_opens_and_reports_plan`
   - create succeeds;
   - info reports selected profile and resolved budget.

2. `durable_create_with_explicit_memory_budget_persists_intent`
   - close/reopen keeps explicit memory cap.

3. `durable_create_with_auto_does_not_persist_host_derived_values`
   - fake host A creates database with auto;
   - fake host B reopens and receives host-B-derived plan.

4. `durable_open_per_open_budget_override_does_not_modify_database_config`
   - per-open override changes current plan only.

5. `cache_open_with_memory_budget_reports_bounded_cache`
   - cache mode reports bounded budget and non-durable mode.

6. `cache_open_without_explicit_budget_uses_auto_conservative_on_unknown_host`
   - unknown host does not get server/unlimited budget.

7. `low_memory_durable_write_fails_before_commit_visibility`
   - oversized operation returns resource error and does not publish partial
     state.

8. `low_memory_cache_write_fails_before_commit_visibility`
   - same for cache mode.

9. `server_profile_increases_budget_without_format_change`
   - server profile opens same format and correctness tests pass.

10. `diagnostics_include_budget_and_pressure_after_rejection`
    - after a resource error, info/health exposes pressure facts.

Acceptance:

1. durable and cache opens use the same resolved budget path;
2. auto-versus-explicit persistence semantics are proven;
3. resource failures are correctness-preserving.

## Executor Tests

Location candidates:

1. `crates/executor-next/tests/runtime_budget_commands.rs`
2. `crates/executor-next/tests/diagnostics_behavior.rs`
3. `crates/executor-next/tests/command_contract.rs`

Required tests:

1. `executor_open_cache_accepts_memory_budget`
   - executor API can open cache with explicit cap.

2. `executor_open_durable_accepts_memory_budget`
   - executor API can open durable with explicit cap.

3. `executor_info_reports_runtime_budget`
   - command output includes selected profile, memory cap, storage budget, and
     source facts.

4. `executor_json_output_keeps_budget_fields_stable`
   - machine-readable output has stable field names and units.

5. `executor_low_memory_mutation_returns_typed_resource_error`
   - error code is stable and includes remediation context.

6. `executor_does_not_expose_manual_flush_compact_as_budget_fix`
   - remediation does not tell normal users to run low-level maintenance.

7. `executor_open_defaults_match_engine_open_defaults`
   - convenience constructors do not bypass runtime planner.

Acceptance:

1. executor does not silently drop open options;
2. output supports agents and SDK bindings;
3. typed errors survive command serialization.

## CLI Contract Tests

Location candidates depend on final CLI crate layout.

Required tests:

1. `strata_new_accepts_profile`
   - `strata new ./db --profile embedded` creates a DB with embedded intent.

2. `strata_new_accepts_memory_budget`
   - `strata new ./db --memory-budget 128MiB` persists explicit intent.

3. `strata_cache_accepts_memory_budget`
   - `strata --cache --memory-budget 64MiB info` reports bounded cache.

4. `strata_info_reports_effective_budget`
   - human output includes selected profile and effective memory budget.

5. `strata_info_json_reports_effective_budget`
   - JSON output includes stable fields and byte values.

6. `invalid_memory_budget_fails_before_open`
   - invalid strings, zero where invalid, overflow, and negative values reject.

7. `noninteractive_missing_budget_prompt_never_occurs`
   - scripts get errors, not prompts.

8. `init_reports_detected_profile_without_requiring_database`
   - first-run setup explains profile policy and local AI separately.

Acceptance:

1. CLI and SDK semantics match;
2. byte parsing is stable;
3. automation receives JSON-friendly errors.

## SDK-Style Conformance Tests

These can start as Rust tests that model binding behavior, then move into
Python/Node binding repositories later.

Required tests:

1. `sdk_open_create_memory_budget_matches_cli_new`
   - SDK create and CLI new produce equivalent database-local intent and
     runtime diagnostics.

2. `sdk_open_cache_memory_budget_matches_cli_cache`
   - SDK cache and CLI cache resolve equivalent budgets.

3. `sdk_open_existing_auto_replans_on_fake_host_change`
   - auto profile adapts when reopened under different fake host facts.

4. `sdk_open_existing_explicit_budget_remains_fixed`
   - explicit persisted budget wins across hosts.

5. `sdk_resource_error_contains_code_limit_and_requested`
   - binding-facing errors include structured facts.

Acceptance:

1. no binding requires `strata init`;
2. SDKs do not depend on CLI-only config;
3. errors are structured enough for applications.

## Derived-State Budget Tests

Location candidates:

1. `crates/engine-next/tests/vector_budget_behavior.rs`
2. `crates/engine-next/tests/search_budget_behavior.rs`
3. `crates/engine-next/tests/graph_budget_behavior.rs`
4. `crates/engine-next/tests/import_export_budget_behavior.rs`
5. `crates/executor-next/tests/inference_budget_behavior.rs`

Required tests:

1. `vector_index_respects_derived_state_budget`
   - low budget degrades, defers, or errors without corrupting vector records.

2. `graph_analytics_respects_scratch_budget`
   - large analytics operation returns bounded error or paginated/deferred
     result.

3. `search_index_rebuild_respects_batch_budget`
   - rebuild does not allocate beyond resolved budget.

4. `import_export_respects_buffer_budget`
   - Arrow or primitive import/export uses bounded batches.

5. `auto_embedding_queue_respects_budget`
   - queued work caps memory and reports pressure.

6. `inference_local_model_admission_reports_model_too_large`
   - if local model memory exceeds configured inference budget, the error is
     typed and actionable.

Acceptance:

1. derived state cannot compromise authored data;
2. degraded features report clear capability/pressure facts;
3. operations remain bounded under embedded profile.

## Source And Dependency Guards

Required guards:

1. storage-next does not import host probing modules.
2. storage-next does not read `/proc/meminfo` or call `sysctl`.
3. storage-next does not classify `embedded`, `desktop`, or `server`.
4. engine-next owns runtime profile classification.
5. executor-next does not bypass engine planner with direct storage budget
   construction.
6. CLI/SDK helpers do not write secrets or auto-derived host facts into
   database-local config.
7. cache product opens do not call `StorageRuntimeBudget::unlimited()`.

## Low-Memory Smoke Tests

Run under small deterministic budgets, not host-dependent memory limits.

Required smoke:

1. cache create/open/info with 16-64 MiB cap;
2. durable create/open/info with 64-128 MiB cap;
3. small KV write/read/list;
4. small JSON set/get;
5. small vector upsert/query;
6. small graph node/edge create/list;
7. small event append/range;
8. oversized write returns resource error before visibility;
9. oversized import/export returns resource error or bounded batching;
10. close/reopen after budget pressure succeeds.

Acceptance:

1. no test relies on actual Pi hardware;
2. all failures are typed;
3. no partial committed state appears after rejection.

## Server Profile Smoke Tests

Run with deterministic fake server profile and normal host resources.

Required smoke:

1. durable create/open under server profile;
2. cache open under server profile still bounded, not unlimited;
3. 100K multi-primitive write/read smoke;
4. background maintenance enabled with larger worker budget;
5. diagnostics report server-selected profile and larger budget;
6. correctness tests match desktop profile behavior.

Acceptance:

1. server profile changes resource envelope only;
2. durable format and correctness do not change;
3. no benchmark-only bypass is introduced.

## Manual Verification

Manual runs after implementation:

```sh
strata new /tmp/strata-edge --memory-budget 64MiB --yes
strata --db /tmp/strata-edge --json info
strata --db /tmp/strata-edge kv put hello world
strata --cache --memory-budget 32MiB --json info
```

Expected:

1. info reports selected profile, memory budget, and sources;
2. cache reports non-durable bounded mode;
3. durable reports durable-local bounded mode;
4. low-budget oversized operations fail with structured resource errors.

## Acceptance Criteria

The test plan passes when:

1. storage, engine, executor, CLI, and SDK-style tests agree on budget
   semantics;
2. cache mode is budgeted by default;
3. auto and explicit persistence semantics are proven;
4. resource diagnostics are visible and machine-readable;
5. low-memory behavior is bounded and correctness-preserving;
6. server profile keeps correctness unchanged while raising limits;
7. source guards prevent regression to storage-owned host probing or hidden
   unlimited cache mode.
