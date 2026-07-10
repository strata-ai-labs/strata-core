# M4-L9 Test Plan: Storage API Boundary

Status: test-suite plan

Parent plan:
`docs/architecture/implementation-plans/m4-l9-storage-api-boundary-implementation-plan.md`

## Goal

Prove that storage-next exposes exactly one synchronous engine-facing storage
boundary, and that the boundary preserves lower-layer correctness without
leaking lower-layer internals or product semantics.

The suite must fail if L9:

1. exposes WAL, manifest, snapshot, table, branch-LSM, commit-runtime,
   lifecycle, backend, object-layout, or service types in public signatures;
2. lets engine-next import storage-next private modules;
3. lets product crates above engine-next import storage-next;
4. exposes async/runtime-specific types;
5. exposes product DTOs or JSON/event/vector/graph/search/intelligence
   semantics;
6. reports retained-history or timestamp-history misses as ordinary not-found;
7. bypasses L7/L8 for commits;
8. bypasses L6/L8 for reads or branch mechanics;
9. exposes durable claims in cache mode;
10. hides durable uncertainty, applied-not-visible, or recovery degradation;
11. allows unsupported distributed/object-durable modes as if they were V1
    production modes;
12. creates a testkit fake that diverges from the production L9 contract.

## Testing Principles

1. Treat L9 as a black-box storage API.
2. Assert structured errors and stable codes, not display strings.
3. Expected read results should come from independent model fixtures or explicit
   committed rows, not from production read-before-read comparisons.
4. Every public method should have closed-runtime and wrong-mode tests.
5. Every lower-layer uncertainty class should survive boundary mapping.
6. Cache and durable mode tests must be separated.
7. Source guards are mandatory tests, not documentation.
8. Fake and faulting persistence must be checked against the same conformance
   suite as production where feasible.
9. Testkit helpers must not reach into private modules from integration tests
   except behind the explicit `testkit` feature.
10. Do not add tests that only verify plan documents; tests should exercise code
    or enforce source boundaries.

## Test Harness Layout

Recommended locations:

1. `crates/storage-next/src/api/tests/` for direct API unit tests.
2. `crates/storage-next/src/testkit/api/` for fake L9 persistence, operation
   scripts, conformance helpers, and fault wrappers.
3. `crates/storage-next/tests/api_conformance.rs` for black-box storage API
   behavior.
4. `crates/storage-next/tests/api_source_guard.rs` for visibility and forbidden
   vocabulary scans.
5. `crates/storage-next/tests/api_snapshot.rs` for public API snapshot or
   signature checks.
6. `crates/storage-next/tests/api_faults.rs` for boundary failure injection.
7. `crates/storage-next/tests/api_properties.rs` for generated operation
   scripts.
8. `crates/engine-next/tests/storage_boundary.rs` for engine compile/integration
   smoke that imports only L9, once the engine-next crate exists.
9. `crates/storage-next/fuzz/fuzz_targets/api_commit_script.rs` for byte-decoded
   commit/read/branch scripts once the API is implemented.
10. `crates/storage-next/fuzz/fuzz_targets/api_fault_script.rs` for boundary
    fault scripts.

Integration tests should import storage-next through the public L9 surface. If a
test needs private lower-layer setup, move it to module-local tests or a
feature-gated testkit helper with a narrow documented purpose.

## Part Gates

Detailed per-slice test plans live under
`docs/architecture/implementation-plans/M4/L9/`.

### Part 1: Boundary Core

Boundary Core closes when tests prove:

1. the crate exports only the intended L9 public surface;
2. lower modules remain private;
3. public boundary types are `#[non_exhaustive]` where future growth is likely;
4. every public error has a stable code;
5. invalid open options fail before lower-layer side effects;
6. cache open does not construct durable services;
7. durable local open/create returns raw recovery and health facts;
8. unsupported object-durable/distributed modes fail with typed unsupported
   errors;
9. close is idempotent after success;
10. operations after close fail with typed closed-state errors.

### Part 2: Data Operations

Data Operations closes when tests prove:

1. latest point reads match committed storage rows;
2. version-bounded reads use the requested retained version;
3. timestamp-bounded reads use timeline resolution and equal-timestamp
   tiebreakers;
4. retained-history reads preserve tombstones and TTL facts;
5. prefix scans are deterministic;
6. range scans are deterministic and bounded;
7. history-unavailable and timestamp-history-unavailable are distinct;
8. commit batches reject malformed keys, empty writes, duplicate mutations, and
   wrong-branch rows;
9. commits return durability class, commit version, timestamp, and visibility
   facts;
10. durable uncertainty and applied-not-visible survive boundary mapping;
11. conflict validation failures are typed conflicts;
12. public transaction sessions are absent;
13. cross-branch atomic commits are absent or explicitly rejected;
14. branch create/fork/delete/clear/list obey generation and pinned-view safety;
15. fork-at-retained-version rejects unretained history.

### Part 3: Operations, Diagnostics, And Engine Testability

Operations and Diagnostics closes when tests prove:

1. checkpoint requests return snapshot/watermark facts without exposing snapshot
   service types;
2. flush requests return table-publication facts without exposing table-object
   service types;
3. compaction/materialization requests return rewrite facts and checkpoint debt;
4. retention/quarantine/purge/repair requests fail closed without current proof;
5. WAL-growth policy facts are observable and product-neutral;
6. diagnostics expose memory/cache/budget/pressure/lazy-read facts;
7. recovery reports expose degraded/failed health without product wording;
8. fake L9 persistence passes the shared conformance suite for supported
   behavior;
9. faulting L9 wrapper can inject every boundary failure family;
10. engine-next can compile and run tests through L9 only.

## Direct Test Matrix

### API Visibility

1. `api_public_surface_exports_runtime_and_options`
2. `api_public_surface_does_not_export_lower_layer_services`
3. `api_public_errors_have_stable_codes`
4. `api_public_enums_are_non_exhaustive`
5. `api_result_type_is_synchronous`
6. `api_no_async_runtime_types_in_public_signatures`
7. `api_testkit_is_hidden_without_testkit_feature`

### Open And Close

1. `open_cache_returns_ephemeral_outcome`
2. `open_cache_does_not_construct_durable_services`
3. `open_durable_local_standard_returns_recovery_facts`
4. `open_durable_local_always_returns_recovery_facts`
5. `open_rejects_object_durable_candidate_in_v1`
6. `open_rejects_distributed_writer_mode`
7. `open_invalid_config_has_no_side_effects`
8. `close_after_open_returns_final_facts`
9. `close_is_idempotent_after_closed`
10. `operation_after_close_returns_closed_error`

### Reads

1. `read_latest_returns_newest_visible_value`
2. `read_latest_returns_none_after_visible_tombstone`
3. `read_at_version_returns_retained_version`
4. `read_at_version_rejects_pruned_history`
5. `read_at_timestamp_resolves_timeline`
6. `read_at_timestamp_rejects_insufficient_history`
7. `read_history_returns_newest_first`
8. `read_history_includes_tombstone_facts`
9. `prefix_scan_is_sorted_by_key`
10. `range_scan_respects_bounds`
11. `scan_limit_is_enforced_without_partial_order_drift`
12. `cache_and_durable_reads_share_visibility_semantics`

### Commits

1. `commit_rejects_empty_batch`
2. `commit_rejects_duplicate_mutation_keys`
3. `commit_rejects_wrong_branch_mutation`
4. `commit_rejects_unknown_branch`
5. `commit_cache_returns_not_durable_visibility_facts`
6. `commit_standard_returns_standard_durability_facts`
7. `commit_always_returns_always_durability_facts`
8. `commit_conflict_reports_conflict_error`
9. `commit_durable_uncertain_maps_to_boundary_error`
10. `commit_applied_not_visible_maps_to_boundary_error`
11. `commit_does_not_expose_transaction_id`
12. `commit_does_not_claim_serializable_isolation`

### Timeline

1. `timeline_timestamp_lookup_uses_newest_version_at_or_before_timestamp`
2. `timeline_duplicate_timestamp_uses_commit_version_tiebreaker`
3. `timeline_version_lookup_returns_commit_timestamp`
4. `timeline_bounds_report_retained_range`
5. `timeline_corruption_maps_to_recovery_diagnostic`

### Branches

1. `branch_create_allocates_generation`
2. `branch_create_duplicate_rejects`
3. `branch_list_is_deterministic`
4. `branch_fork_current_captures_retained_frontier`
5. `branch_fork_at_retained_version_succeeds`
6. `branch_fork_at_pruned_version_rejects`
7. `branch_clear_rejects_pinned_view`
8. `branch_delete_rejects_pinned_view`
9. `branch_generation_mismatch_rejects_mutation`
10. `branch_delete_removes_from_public_listing`

### Maintenance And Diagnostics

1. `checkpoint_returns_watermark_facts`
2. `flush_returns_publication_facts`
3. `compaction_returns_rewrite_facts`
4. `materialization_returns_rewrite_facts`
5. `retention_requires_current_proof`
6. `quarantine_repair_returns_health_facts`
7. `wal_growth_policy_reports_deferred_when_not_needed`
8. `diagnostics_report_memory_budget_facts`
9. `diagnostics_report_cache_facts`
10. `diagnostics_report_lazy_read_facts`
11. `diagnostics_do_not_use_product_vocabulary`

## Source Guard Tests

`api_source_guard.rs` must check:

1. `src/lib.rs` exports `api` intentionally and does not expose lower modules as
   public production modules;
2. `src/api/**` does not import engine, intelligence, inference, executor, CLI,
   SDK, or StrataHub crates;
3. `src/api/**` does not import raw backend, layout, format, service, table, or
   branch internals except through approved crate-private adapters;
4. lower modules do not import `crate::api`;
5. production L9 code contains no `async`, `Future`, tokio, async-std, channel
   runtime, or thread-spawn ownership;
6. production L9 code contains no product vocabulary for JSON, event, graph,
   vector, embedding, prompt, model, chat, or search semantics;
7. engine-next does not import `storage_next::branch`, `storage_next::commit`,
   `storage_next::lifecycle`, `storage_next::service`, `storage_next::format`,
   `storage_next::table`, `storage_next::backend`, or `storage_next::layout`;
8. product crates above engine-next do not import storage-next;
9. testkit fake/faulting helpers are behind `cfg(test)` or `feature = "testkit"`;
10. public fixtures do not use planning slice labels in Rust identifiers or test
    corpus bytes.

## Generated Conformance

The generated L9 script should decode input bytes into operations:

1. open mode;
2. branch create/fork/delete/clear;
3. commit put/delete/tombstone/TTL;
4. point read latest/version/timestamp;
5. history read;
6. prefix/range scan;
7. checkpoint/flush/maintenance trigger;
8. retention/quarantine/purge/repair request;
9. close/reopen;
10. injected fault.

The model should be independent of production storage internals. It may model a
single branch first, but closeout requires branch fork/delete and retained
history behavior before L9 is complete.

Required counters:

1. cache open path;
2. durable open path;
3. read latest;
4. read retained version;
5. read timestamp;
6. retained-history miss;
7. commit success;
8. commit conflict;
9. durable uncertainty;
10. branch lifecycle;
11. checkpoint/flush;
12. retention/quarantine/repair;
13. close and post-close rejection.

Generated coverage must prove input-derived operations reached each required
family. Canonical smoke scripts may exist, but they cannot be the only reason a
property counter is nonzero.

## Fake And Faulting Persistence

The fake L9 persistence implementation used by engine-next tests must support:

1. deterministic branch row state;
2. latest/version/timestamp/history reads;
3. prefix/range scans;
4. commit version allocation;
5. branch create/fork/delete/clear mechanics;
6. configurable conflicts;
7. configurable retained-history bounds;
8. configurable recovery health facts;
9. configurable backend capability facts.

The faulting wrapper must inject:

1. read failure;
2. write validation failure;
3. write failure before storage mutation;
4. ambiguous write outcome;
5. durable-but-not-visible outcome;
6. post-commit failure;
7. recovery degradation;
8. maintenance failure;
9. close failure;
10. retained-history loss.

Both fake and faulting surfaces should implement the same boundary trait or
contract used by production L9. Engine tests should not depend on a separate
storage shape.

## Sensitivity Probes

Record each probe in
`docs/architecture/implementation-plans/M4/L9/m4-l9-porting-log.md` with:

1. mutation description;
2. mutated file and line;
3. expected failing test;
4. actual failing test;
5. result.

Required probes:

1. expose a lower-layer type in an API signature;
2. import a private storage module from engine-next;
3. convert retained-history miss to not-found;
4. drop durable uncertainty from commit outcome;
5. claim durable facts in cache mode;
6. bypass conflict validation;
7. ignore branch generation mismatch;
8. allow delete with a pinned view;
9. allow unsupported object-durable mode;
10. add product vocabulary to L9 production code;
11. add async runtime type to L9 production code;
12. make fake persistence diverge from production error mapping.

## Closeout Commands

Minimum command matrix:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --locked --test api_source_guard
cargo test -p strata-storage-next --locked --test api_conformance
cargo test -p strata-storage-next --features testkit --locked --test api_conformance
cargo test -p strata-storage-next --features testkit --locked --test api_properties
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test api_faults
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

The porting log must record pass/fail for each command. If a pre-existing
failure remains outside the current slice, record the exact failing command,
reason, and owning follow-up.
Add the engine-next boundary smoke command when the engine-next crate exists in
the workspace.
