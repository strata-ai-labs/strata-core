# V1 Test Inventory

Status: M0TE populated inventory

## Purpose

This file is the canonical M0TE inventory of existing test files and their V1
disposition.

Existing tests are evidence, not authority. A file marked `Keep` or `Rewrite`
still has to satisfy the V1 engineering standards when it is ported. A file
marked `Archive` or `Delete` must not constrain V1 behavior.

## Scope

This inventory covers 212 files:

1. root test files and test support under `tests/`;
2. crate integration tests under `crates/*/tests/`;
3. explicit source test modules under `crates/*/src/**/tests/`;
4. benchmark source, baseline, history, and result files under `benchmarks/`,
   excluding generated build output under `benchmarks/target/`;
5. current benchmark files under `crates/*/benches/`;
6. fixture and proptest-regression files consumed by those tests.

Inline unit tests embedded inside ordinary production source files are not
listed one-by-one here. They move with their owning module during M1-M10 and
must follow the same action as that module's V1 owner.

## Classification Legend

| Field | Values |
|---|---|
| `v1_decision` | Required, Optional, Redesign, Remove, Evidence-only |
| `action` | Keep, Rewrite, Archive, Delete |
| `target_track` | `M1T` through `M10T`, or `none` for archive/delete/support-only rows |
| `target_epic` | Planned test epic when known, otherwise `none` |

## Summary

| Area | Default disposition |
|---|---|
| Core identifier and public-surface guards | Rewrite into M1/M5/M9 guard tests. |
| Storage durability, recovery, branch visibility, and segmented-store tests | Rewrite into M3/M4 storage-next conformance, property, and fault tests. |
| Engine primitive, branch, relationship, versioning, and retrieval tests | Rewrite into M5/M6 engine-next conformance and product-path tests. |
| Intelligence search and model-orchestration tests | Rewrite into M8 intelligence-next tests. |
| Executor, CLI, IPC, and command tests | Rewrite into M9 cutover tests, except removed commands. |
| Follower mode, branch tags/notes, public transaction sessions, branch bundles, and normal-user maintenance workflows | Delete or archive; V1 guard tests must prove absence. |
| Stress, adversarial, and benchmark files | Rewrite into M10 readiness/performance gates. |

## Root Test Inventory

| path | current_area | behavior | v1_decision | action | target_track | target_epic | fixtures | reason | notes |
|---|---|---|---|---|---|---|---|---|---|
| `tests/archive/README.md` | archive metadata | Archive directory policy | Evidence-only | Keep | none | none | none | Documents archive semantics. | Not executable. |
| `tests/cli_external_suite.py` | CLI external | Installed-binary CLI, shell, pipe, REPL behavior | Required | Rewrite | `M9T` | `M9TA`, `M9TC` | `tests/cli_external_suite_manifest.json` | CLI survives but command surface changes. | Remove maintenance, transaction, tag/note, follower, and branch-bundle cases. |
| `tests/cli_external_suite_manifest.json` | CLI external fixture | External CLI case manifest | Redesign | Rewrite | `M9T` | `M9TA` | none | Manifest must match V1 CLI. | Contains removed `compact` and `flush` cases today. |
| `tests/common/branching.rs` | shared test support | Branch lifecycle and lineage helpers | Redesign | Rewrite | `M6T` | `M6TA` | none | Helper imports old engine internals and graph DAG names. | Keep intent; rename around V1 branch lifecycle. |
| `tests/common/mod.rs` | shared test support | Root integration helpers and database fixtures | Redesign | Rewrite | `M6T` | `M6TA` | root fixtures | Helper opens old runtime subsystems directly. | V1 testkit helpers must use product open surfaces. |
| `tests/core_foundation_surface.rs` | core guard | Core public surface and dependency shape | Required | Rewrite | `M1T` | `M1TA` | none | Core-next surface changes. | Guard new core atoms and no upward dependencies. |
| `tests/durability/crash_recovery.rs` | durability | Crash recovery across durable transitions | Required | Rewrite | `M4T` | `M4TD` | none | Recovery behavior survives under storage-next. | Move to crash harness/fault-window model. |
| `tests/durability/cross_primitive_recovery.rs` | durability | Recovery across engine capabilities | Required | Rewrite | `M6T` | `M6TA` | none | Behavior survives but engine/storage boundary changes. | Split storage recovery from engine product replay. |
| `tests/durability/main.rs` | durability harness | Root durability module harness | Redesign | Rewrite | `M4T` | `M4TA` | durability modules | Harness must match new storage-next tests. | Rename around storage behavior. |
| `tests/durability/mode_equivalence.rs` | durability | Cache/standard/always mode equivalence | Required | Rewrite | `M4T` | `M4TE` | none | Three durability modes remain. | Cache has no WAL; update assertions. |
| `tests/durability/recovery_invariants.rs` | durability | Recovery invariant coverage | Required | Rewrite | `M4T` | `M4TD` | none | Invariants survive. | Port to storage health/error contracts. |
| `tests/durability/snapshot_lifecycle.rs` | durability | Snapshot lifecycle behavior | Required | Rewrite | `M3T` | `M3TD` | none | Snapshot substrate survives with new L3/L4 format. | Golden bytes must be refreshed. |
| `tests/durability/stress.rs` | durability | Long-running durability stress | Required | Rewrite | `M10T` | `M10TA` | none | Stress belongs in readiness gate. | Keep as late gate, not early CI blocker. |
| `tests/durability/wal_lifecycle.rs` | durability | WAL lifecycle behavior | Required | Rewrite | `M3T` | `M3TD` | none | WAL service survives in durable modes. | Cache path must assert WAL absence. |
| `tests/engine/acid_concurrent.rs` | engine product | Concurrent transaction-style guarantees | Redesign | Rewrite | `M6T` | `M6TA` | none | Public ACID/manual sessions are not V1 product claims. | Recast as atomic write/batch and isolation behavior. |
| `tests/engine/acid_properties.rs` | engine product | ACID property assertions | Redesign | Rewrite | `M6T` | `M6TA` | none | Product promise changes to explicit write semantics. | Avoid public ACID overclaim. |
| `tests/engine/adversarial.rs` | engine stress | Adversarial product behavior | Required | Rewrite | `M10T` | `M10TB` | none | High-value hardening coverage. | Port after engine product surface stabilizes. |
| `tests/engine/adversarial_deep.rs` | engine stress | Deep adversarial product behavior | Required | Rewrite | `M10T` | `M10TB` | none | High-value late readiness coverage. | Keep out of early fast suite. |
| `tests/engine/branch_isolation.rs` | engine branch | Branch isolation behavior | Required | Rewrite | `M6T` | `M6TC` | none | Branch isolation is V1 core product behavior. | Assert through V1 branch API. |
| `tests/engine/cross_primitive.rs` | engine capabilities | Cross-capability behavior | Required | Rewrite | `M6T` | `M6TA` | none | Cross-capability behavior survives. | Recast as KV-backed capability contract. |
| `tests/engine/database/durability_modes.rs` | engine database | Engine open behavior for durability modes | Required | Rewrite | `M5T` | `M5TA` | none | Engine lifecycle survives but storage API changes. | Coordinate with M4 durability tests. |
| `tests/engine/database/lifecycle.rs` | engine database | Database lifecycle behavior | Required | Rewrite | `M5T` | `M5TA` | none | Product open/lifecycle remains. | Use engine-next lifecycle surface. |
| `tests/engine/database/mod.rs` | engine database harness | Engine database module harness | Redesign | Rewrite | `M5T` | `M5TA` | database modules | Harness must match engine-next crate shape. | No old subsystem construction. |
| `tests/engine/database/transactions.rs` | engine database | Database transaction behavior | Redesign | Rewrite | `M6T` | `M6TA` | none | Manual transaction sessions are removed. | Keep only atomic write/batch intent. |
| `tests/engine/main.rs` | engine harness | Root engine module harness | Redesign | Rewrite | `M6T` | `M6TA` | engine modules | Harness must match V1 modules. | Rename behavior-focused modules. |
| `tests/engine/p0_concurrency.rs` | engine concurrency | Engine concurrency stress | Required | Rewrite | `M10T` | `M10TA` | none | Concurrency remains high-value readiness coverage. | Port to deterministic scheduler where practical. |
| `tests/engine/primitives/branchindex.rs` | engine primitive | Branch index behavior | Redesign | Rewrite | `M6T` | `M6TC` | none | Branch metadata layout changes. | Move to branch capability tests. |
| `tests/engine/primitives/eventlog.rs` | engine primitive | Event capability behavior | Required | Rewrite | `M6T` | `M6TB` | none | Event capability survives. | Assert V1 append/divergence rules. |
| `tests/engine/primitives/jsonstore.rs` | engine primitive | JSON capability behavior | Required | Rewrite | `M6T` | `M6TB` | `tests/fixtures/dirty_jsonstore_data.json` | JSON capability survives. | Use document-level V1 merge baseline. |
| `tests/engine/primitives/kv.rs` | engine primitive | KV capability behavior | Required | Rewrite | `M6T` | `M6TB` | `tests/fixtures/kv_*` | KV is core capability. | Port to V1 public API. |
| `tests/engine/primitives/mod.rs` | engine primitive harness | Primitive module harness | Redesign | Rewrite | `M6T` | `M6TB` | primitive modules | Harness must match V1 capability modules. | No old primitive vocabulary if renamed. |
| `tests/engine/primitives/vectorstore.rs` | engine primitive | Vector capability behavior | Required | Rewrite | `M6T` | `M6TB` | none | Vector capability survives. | Include temporal refusal rule. |
| `tests/engine/stress.rs` | engine stress | Engine stress behavior | Required | Rewrite | `M10T` | `M10TB` | none | Readiness stress remains useful. | Port after V1 API stabilizes. |
| `tests/engine_consolidation_search_characterization.rs` | consolidation evidence | Search behavior after old consolidation | Evidence-only | Archive | none | none | none | Cleanup-era characterization should not constrain V1 shape. | Reuse behavior in M6/M8 tests as needed. |
| `tests/engine_consolidation_vector_characterization.rs` | consolidation evidence | Vector behavior after old consolidation | Evidence-only | Archive | none | none | none | Cleanup-era characterization should not constrain V1 shape. | Reuse behavior in M6 tests as needed; do not port follower product-open cases. |
| `tests/engine_error_surface.rs` | engine guard | Engine error surface | Required | Rewrite | `M5T` | `M5TB` | none | Error types and codes change. | Assert V1 structured diagnostics. |
| `tests/engine_security_surface.rs` | engine guard | Engine security/public surface | Required | Rewrite | `M5T` | `M5TA` | none | Public surface guard remains. | Remove `OpenOptions::follower` coverage; update to V1 exports and access modes. |
| `tests/engine_surface_imports.rs` | engine guard | Engine dependency/public import surface | Required | Rewrite | `M5T` | `M5TA` | none | Boundary guard remains. | Update for engine-next modules. |
| `tests/event_runtime_surface.rs` | engine guard | Event runtime public surface | Required | Rewrite | `M6T` | `M6TB` | none | Event capability remains. | Move to capability conformance. |
| `tests/executor/adversarial.rs` | executor | Command adversarial behavior | Required | Rewrite | `M9T` | `M9TA` | none | Executor remains command boundary. | Delete public transaction-command cases; preserve adversarial command intent. |
| `tests/executor/branch_invariants.rs` | executor | Branch command invariants | Required | Rewrite | `M9T` | `M9TA` | none | Branch commands remain. | Align with V1 branch semantics. |
| `tests/executor/branch_metadata_commands.rs` | executor | Branch tags and notes commands | Remove | Delete | none | none | none | Tags/notes are removed from V1. | Replace with removed-command guards in M9. |
| `tests/executor/command_dispatch.rs` | executor | Command dispatch behavior | Required | Rewrite | `M9T` | `M9TA` | none | Command boundary remains. | Remove branch-bundle and maintenance command cases while updating to V1 command set. |
| `tests/executor/common.rs` | executor support | Executor test helpers | Redesign | Rewrite | `M9T` | `M9TA` | none | Helpers construct old surfaces. | Keep helper intent. |
| `tests/executor/error_handling.rs` | executor | Command error behavior | Required | Rewrite | `M9T` | `M9TA` | none | Errors remain but codes/classes change. | Delete public transaction error cases; assert V1 wire errors. |
| `tests/executor/indexing_auto_embed_commands.rs` | executor | Autoembedding/indexing commands | Required | Rewrite | `M8T` | `M8TA` | none | Intelligence substrate survives. | Rewrite `TxnBegin`/`TxnCommit` setup to V1 atomic writes; command surface may move in M9. |
| `tests/executor/main.rs` | executor harness | Executor module harness | Redesign | Rewrite | `M9T` | `M9TA` | executor modules | Harness must match V1 command modules. | Remove transaction/tag/note modules. |
| `tests/executor/product_surface_commands.rs` | executor | Product command surface | Required | Rewrite | `M9T` | `M9TA` | none | High-value product-path coverage. | Update to V1 product surface. |
| `tests/executor/retention_contract.rs` | executor | Retention command behavior | Redesign | Rewrite | `M6T` | `M6TD` | none | Internal retention remains; normal-user maintenance removed. | Rewrite public transaction setup to V1 atomic/batch harnesses; convert `Flush`/`Compact` command checks into removed-surface guards. |
| `tests/executor/serialization.rs` | executor | Command serialization | Required | Rewrite | `M9T` | `M9TA` | none | Wire/command encoding remains. | Remove `TxnBegin`/`TxnCommit` golden cases; refresh V1 command shape. |
| `tests/executor/session_transactions.rs` | executor | Public session transaction commands | Remove | Delete | none | none | none | Public manual transaction sessions are removed. | Add absence guards in M9. |
| `tests/executor/storage_boundary_characterization.rs` | executor | Executor/engine storage-boundary behavior | Redesign | Rewrite | `M9T` | `M9TE` | none | Boundary remains but transaction/session parts change. | Split removed transaction cases. |
| `tests/executor_runtime.rs` | executor | Runtime command behavior | Required | Rewrite | `M9T` | `M9TA` | none | Executor runtime survives as command layer. | Remove follower-open assertions, public transaction cases, and locked-without-socket `--follower` message; update IPC/product open assumptions. |
| `tests/fixtures/branch_bundle/legacy_owner_v2.branchbundle.tar.zst` | fixture | Legacy branch-bundle artifact | Remove | Delete | none | none | none | Branch bundles are removed. | Dataset clone artifacts replace this direction. |
| `tests/fixtures/dirty_jsonstore_data.json` | fixture | JSON malformed/edge data | Required | Keep | `M6T` | `M6TB` | none | Useful JSON capability fixture. | Verify no secrets before reuse. |
| `tests/fixtures/eventlog_test_data.jsonl` | fixture | Event capability data | Required | Keep | `M6T` | `M6TB` | none | Useful event fixture. | Large fixture; keep only if M6 tests need it. |
| `tests/fixtures/kv_edge_cases.jsonl` | fixture | KV edge cases | Required | Keep | `M6T` | `M6TB` | none | Useful KV fixture. | Keep with V1 key/value validation. |
| `tests/fixtures/kv_test_data.json` | fixture | KV sample data | Required | Keep | `M6T` | `M6TB` | none | Useful KV fixture. | Keep if non-redundant. |
| `tests/fixtures/kv_test_data.jsonl` | fixture | KV sample data | Required | Keep | `M6T` | `M6TB` | none | Useful KV fixture. | Keep if non-redundant. |
| `tests/fixtures/statecell_test_data.jsonl` | fixture | Legacy statecell-style data | Evidence-only | Archive | none | none | none | Statecell vocabulary is not V1 product language. | Reuse data only if renamed around V1 behavior. |
| `tests/integration/branching.rs` | integration | Broad branch operations | Required | Rewrite | `M6T` | `M6TC` | none | Branching is V1 signature behavior. | Split oversized file into focused product paths. |
| `tests/integration/branching_adversarial_history.rs` | integration | Branch history property/adversarial behavior | Required | Rewrite | `M6T` | `M6TC` | `tests/proptest-regressions/branching_adversarial_history.txt` | History behavior survives. | Move property cases to V1 branch model tests; delete follower failure cases. |
| `tests/integration/branching_control_store_recovery.rs` | integration | Branch control-store recovery | Required | Rewrite | `M5T` | `M5TC` | none | Control plane survives with new layout. | Use V1 system branch/system-space contract. |
| `tests/integration/branching_convergence_differential.rs` | integration | Branch convergence differential behavior | Required | Rewrite | `M6T` | `M6TC` | none | Promotion/compare behavior survives. | Delete follower publish-clamp smoke check; align surviving branch convergence with Strict/SourceWins semantics. |
| `tests/integration/branching_degraded_primitive_paths.rs` | integration | Branch paths under degraded capabilities | Required | Rewrite | `M6T` | `M6TC` | none | Degradation handling survives. | Assert V1 error classes. |
| `tests/integration/branching_gc_quarantine_recovery.rs` | integration | Branch GC/quarantine recovery | Required | Rewrite | `M4T` | `M4TD` | none | Storage recovery/retention survives. | Move storage mechanics below engine. |
| `tests/integration/branching_generation_migration.rs` | integration | Branch generation migration behavior | Evidence-only | Archive | none | none | none | Pre-V1 migration and branch-bundle behavior are not V1 product paths. | Preserve only as history. |
| `tests/integration/branching_guardrails.rs` | integration | Branch operation guardrails | Required | Rewrite | `M6T` | `M6TC` | none | Guardrails survive. | Delete tag/note guardrails; assert V1 public semantics for surviving operations. |
| `tests/integration/branching_lifecycle_gate.rs` | integration | Branch lifecycle gates | Required | Rewrite | `M6T` | `M6TC` | none | Lifecycle gating survives. | Delete tag/note and branch-bundle cases; keep lifecycle gates for surviving operations. |
| `tests/integration/branching_lifecycle_restart.rs` | integration | Lifecycle restart recovery | Required | Rewrite | `M5T` | `M5TC` | none | Control-plane recovery survives. | Delete tag/note and branch-bundle restart cases; use V1 control-plane rows. |
| `tests/integration/branching_merge_lineage_edges.rs` | integration | Merge lineage edge behavior | Required | Rewrite | `M6T` | `M6TC` | none | Lineage remains; projection shape changes. | Authoritative lineage is control-plane, not graph-only. |
| `tests/integration/branching_recreate_state_machine.rs` | integration | Branch recreate state machine | Required | Rewrite | `M6T` | `M6TC` | none | Delete/recreate semantics survive. | Align with destructive V1 delete. |
| `tests/integration/branching_retention_matrix.rs` | integration | Branch retention matrix | Required | Rewrite | `M4T` | `M4TC` | none | Retention mechanics survive. | Public maintenance commands do not. |
| `tests/integration/branching_retention_state_machine.rs` | integration | Branch retention property/state machine | Required | Rewrite | `M4T` | `M4TC` | `tests/proptest-regressions/branching_retention_state_machine.txt` | Retention behavior survives. | Move to storage/engine boundary tests. |
| `tests/integration/branching_same_name_race.rs` | integration | Same-name branch race behavior | Required | Rewrite | `M6T` | `M6TC` | none | Race guard survives. | Use deterministic concurrency harness. |
| `tests/integration/data/branching_shape_inventory.json` | integration fixture | Branch shape inventory | Redesign | Rewrite | `M6T` | `M6TC` | none | Branch shape vocabulary changes. | Refresh as V1 golden only if needed. |
| `tests/integration/main.rs` | integration harness | Integration module harness | Redesign | Rewrite | `M6T` | `M6TA` | integration modules | Harness must match V1 module set. | Split by product pathway. |
| `tests/integration/merge_base_characterization.rs` | integration | Merge-base characterization | Required | Rewrite | `M6T` | `M6TC` | none | Merge-base behavior survives. | Rename away from characterization if kept. |
| `tests/integration/modes.rs` | integration | Runtime/durability mode behavior | Required | Rewrite | `M4T` | `M4TE` | none | Modes survive. | Assert cache/standard/always V1 semantics. |
| `tests/integration/primitives.rs` | integration | Cross-primitive product behavior | Required | Rewrite | `M6T` | `M6TB` | root fixtures | Capabilities survive. | Recast as KV-backed capabilities. |
| `tests/integration/recovery_cross_crate.rs` | integration | Cross-crate recovery behavior | Required | Rewrite | `M5T` | `M5TB` | none | Recovery boundary survives. | Engine should consume storage through L9 only. |
| `tests/integration/scale.rs` | integration | Scale behavior | Required | Rewrite | `M10T` | `M10TD` | none | Scale coverage belongs in readiness gate. | Refresh thresholds. |
| `tests/intelligence/architectural_invariants.rs` | intelligence | Intelligence boundary invariants | Required | Rewrite | `M8T` | `M8TA` | none | Boundary survives. | Ensure no storage imports/provider leakage. |
| `tests/intelligence/budget_semantics.rs` | intelligence | Retrieval/model budget behavior | Required | Rewrite | `M8T` | `M8TA` | none | Budget semantics survive. | Use runtime resource profile contract. |
| `tests/intelligence/explainability.rs` | intelligence | Search explanation behavior | Optional | Rewrite | `M8T` | `M8TD` | none | Diagnostics survive if exposed. | Align with stage diagnostics. |
| `tests/intelligence/fusion.rs` | intelligence | Fusion behavior | Required | Rewrite | `M8T` | `M8TD` | none | Hybrid search survives. | Use recipe-owned knobs. |
| `tests/intelligence/hybrid.rs` | intelligence | Hybrid retrieval behavior | Required | Rewrite | `M8T` | `M8TD` | none | Hybrid search survives. | Coordinate with engine retrieval substrate. |
| `tests/intelligence/identity.rs` | intelligence | Search identity behavior | Required | Rewrite | `M8T` | `M8TA` | none | Entity identity in results survives. | Use V1 EntityRef grammar. |
| `tests/intelligence/indexing.rs` | intelligence | Indexing/search derived state | Required | Rewrite | `M8T` | `M8TB` | none | Derived-state behavior survives. | Align with engine freshness manifests. |
| `tests/intelligence/issue_018_search_overfetch.rs` | intelligence | Search overfetch regression | Required | Rewrite | `M8T` | `M8TD` | none | Ranking/retrieval bug coverage remains useful. | Rename issue-based test if kept. |
| `tests/intelligence/m6_budget_propagation.rs` | intelligence | Old milestone budget propagation | Required | Rewrite | `M8T` | `M8TA` | none | Behavior survives, name does not. | Rename away from `m6`. |
| `tests/intelligence/m6_hybrid_search.rs` | intelligence | Old milestone hybrid search | Required | Rewrite | `M8T` | `M8TD` | none | Hybrid search survives, name does not. | Rename away from `m6`. |
| `tests/intelligence/m6_rrf_fusion.rs` | intelligence | Old milestone RRF fusion | Required | Rewrite | `M8T` | `M8TD` | none | RRF behavior survives, name does not. | Rename away from `m6`. |
| `tests/intelligence/m6_search_request.rs` | intelligence | Old milestone search request DTO | Redesign | Rewrite | `M8T` | `M8TA` | none | DTO shape changes. | Rename away from `m6`. |
| `tests/intelligence/m6_search_response.rs` | intelligence | Old milestone search response DTO | Redesign | Rewrite | `M8T` | `M8TA` | none | DTO shape changes. | Rename away from `m6`. |
| `tests/intelligence/main.rs.deferred` | intelligence harness | Deferred intelligence harness | Redesign | Rewrite | `M8T` | `M8TA` | intelligence modules | Harness is deferred and contains old milestone modules. | Revive only with V1 names. |
| `tests/intelligence/scoring.rs` | intelligence | Search scoring behavior | Required | Rewrite | `M8T` | `M8TD` | none | Scoring survives. | Align with recipe/stage diagnostics. |
| `tests/intelligence/search_all_primitives.rs` | intelligence | Search across capabilities | Required | Rewrite | `M8T` | `M8TD` | none | Cross-capability search survives. | Use capability abstraction. |
| `tests/intelligence/search_backend_tiebreak.rs` | intelligence | Search backend tiebreak behavior | Required | Rewrite | `M8T` | `M8TD` | none | Deterministic results survive. | Rename backend wording if needed. |
| `tests/intelligence/search_budget_enforcement.rs` | intelligence | Search budget enforcement | Required | Rewrite | `M8T` | `M8TA` | none | Budgets survive. | Tie to runtime profile. |
| `tests/intelligence/search_budget_enforcement_cross.rs` | intelligence | Cross-stage budget enforcement | Required | Rewrite | `M8T` | `M8TA` | none | Budgets survive. | Tie to runtime profile. |
| `tests/intelligence/search_correctness.rs` | intelligence | Search correctness | Required | Rewrite | `M8T` | `M8TD` | none | Core retrieval correctness survives. | Split by stage where useful. |
| `tests/intelligence/search_deterministic_order.rs` | intelligence | Search order determinism | Required | Rewrite | `M8T` | `M8TD` | none | Determinism survives. | Keep as fast regression. |
| `tests/intelligence/search_dimension_match.rs` | intelligence | Vector dimension match behavior | Required | Rewrite | `M8T` | `M8TC` | none | Embedding model/dimension validation is V1 required. | Add model-mismatch error code assertions. |
| `tests/intelligence/search_facade_tiebreak.rs` | intelligence | Search public-wrapper tiebreak behavior | Required | Rewrite | `M8T` | `M8TD` | none | Behavior survives, `facade` name does not. | Rename. |
| `tests/intelligence/search_hybrid_orchestration.rs` | intelligence | Hybrid orchestration | Required | Rewrite | `M8T` | `M8TD` | none | Orchestration survives. | Use repeatable stage outcome. |
| `tests/intelligence/search_no_normalization.rs` | intelligence | Score normalization behavior | Required | Rewrite | `M8T` | `M8TD` | none | Score behavior survives. | Verify recipe ownership. |
| `tests/intelligence/search_readonly.rs` | intelligence | Search readonly behavior | Required | Rewrite | `M8T` | `M8TD` | none | Readonly retrieval survives. | Coordinate with IPC readonly clients. |
| `tests/intelligence/search_score_normalization.rs` | intelligence | Score normalization behavior | Required | Rewrite | `M8T` | `M8TD` | none | Score behavior survives. | Verify recipe ownership. |
| `tests/intelligence/search_single_threaded.rs` | intelligence | Single-threaded search behavior | Required | Rewrite | `M8T` | `M8TD` | none | Deterministic local behavior survives. | Tie to resource profiles if needed. |
| `tests/intelligence/search_snapshot_consistency.rs` | intelligence | Search snapshot consistency | Required | Rewrite | `M8T` | `M8TD` | none | Snapshot consistency survives. | Use timeline/freshness contract. |
| `tests/intelligence/stress.rs` | intelligence | Intelligence/search stress | Required | Rewrite | `M10T` | `M10TB` | none | Stress belongs in readiness gate. | Port after M8 stabilizes. |
| `tests/proptest-regressions/branching_adversarial_history.txt` | proptest corpus | Branch history regression seed | Required | Keep | `M6T` | `M6TC` | none | Keep if corresponding property test is ported. | Delete only if property test is deleted. |
| `tests/proptest-regressions/branching_retention_state_machine.txt` | proptest corpus | Branch retention regression seed | Required | Keep | `M4T` | `M4TC` | none | Keep if corresponding property test is ported. | Delete only if property test is deleted. |
| `tests/storage/branch_isolation.rs` | storage | Storage branch isolation | Required | Rewrite | `M4T` | `M4TB` | none | Branch-aware storage survives. | Port to storage-next row model. |
| `tests/storage/main.rs` | storage harness | Storage module harness | Redesign | Rewrite | `M4T` | `M4TA` | storage modules | Harness must match storage-next crate. | Replace old module names. |
| `tests/storage/mvcc_invariants.rs` | storage | Version visibility invariants | Required | Rewrite | `M4T` | `M4TC` | none | Commit visibility survives. | Use V1 timeline/version model. |
| `tests/storage/snapshot_isolation.rs` | storage | Snapshot isolation behavior | Required | Rewrite | `M4T` | `M4TC` | none | Internal isolation remains. | Avoid public transaction wording. |
| `tests/storage/stress.rs` | storage | Storage stress | Required | Rewrite | `M10T` | `M10TA` | none | Stress belongs in readiness gate. | Port after storage-next completes. |
| `tests/storage_surface_imports.rs` | boundary guard | Storage import/public surface guard | Required | Rewrite | `M5T` | `M5TA` | none | Boundary guard remains. | Update for storage-next and engine-next. |
| `tests/transaction_runtime/cas_operations.rs` | transaction runtime | CAS-like write semantics | Redesign | Rewrite | `M6T` | `M6TA` | none | Public transaction runtime removed; atomic writes remain. | Recast as batch/write semantics. |
| `tests/transaction_runtime/concurrent_transactions.rs` | transaction runtime | Concurrent commit conflict behavior | Redesign | Rewrite | `M4T` | `M4TC` | none | Internal commit ordering remains. | Move below public transaction surface. |
| `tests/transaction_runtime/conflict_detection.rs` | transaction runtime | Conflict detection | Redesign | Rewrite | `M4T` | `M4TC` | none | Internal conflict/refusal behavior remains. | Tie to branch generation and visible versions. |
| `tests/transaction_runtime/main.rs` | transaction runtime harness | Transaction runtime module harness | Redesign | Archive | none | none | transaction modules | Public transaction runtime is removed. | Surviving invariants move to M4/M6 tests. |
| `tests/transaction_runtime/manager_commit.rs` | transaction runtime | Transaction manager commit behavior | Redesign | Archive | none | none | none | Manager-shaped public runtime is not V1 vocabulary. | Reuse internal commit intent in M4. |
| `tests/transaction_runtime/occ_invariants.rs` | transaction runtime | OCC invariants | Redesign | Rewrite | `M4T` | `M4TC` | none | Optimistic commit mechanics survive internally. | Assert storage commit semantics, not public sessions. |
| `tests/transaction_runtime/snapshot_isolation.rs` | transaction runtime | Snapshot isolation | Redesign | Rewrite | `M4T` | `M4TC` | none | Internal isolation survives. | Remove manual transaction API assumptions. |
| `tests/transaction_runtime/stress.rs` | transaction runtime | Transaction runtime stress | Redesign | Rewrite | `M10T` | `M10TA` | none | Concurrency stress remains useful. | Rebuild over V1 write/batch semantics. |
| `tests/transaction_runtime/transaction_lifecycle.rs` | transaction runtime | Public transaction lifecycle | Remove | Delete | none | none | none | Public begin/commit/rollback sessions removed. | Add removed-surface guard in M9. |
| `tests/transaction_runtime/transaction_states.rs` | transaction runtime | Public transaction state machine | Remove | Delete | none | none | none | Public transaction states removed. | Internal state tests move under storage/engine. |
| `tests/transaction_runtime/version_counter.rs` | transaction runtime | Version allocation behavior | Required | Rewrite | `M4T` | `M4TC` | none | Commit version allocation survives. | Move to storage commit timeline tests. |

## Crate Test Inventory

| path | current_area | behavior | v1_decision | action | target_track | target_epic | fixtures | reason | notes |
|---|---|---|---|---|---|---|---|---|---|
| `crates/engine/src/database/tests/checkpoint.rs` | engine inline tests | Checkpoint behavior | Required | Rewrite | `M4T` | `M4TD` | none | Checkpoint mechanics move to storage lifecycle. | Delete follower open/checkpoint cases; public maintenance command path removed. |
| `crates/engine/src/database/tests/codec.rs` | engine inline tests | Database codec behavior | Required | Rewrite | `M3T` | `M3TA` | none | Durable bytes need new golden coverage. | Delete follower codec cases; move surviving format assertions to storage-next. |
| `crates/engine/src/database/tests/contention.rs` | engine inline tests | Open/contention behavior | Required | Rewrite | `M5T` | `M5TA` | none | Single-writer/IPC behavior survives. | Align with no follower mode. |
| `crates/engine/src/database/tests/mod.rs` | engine inline harness | Database test harness | Redesign | Rewrite | `M5T` | `M5TA` | database test modules | Harness follows old module shape. | Split by lifecycle/recovery/format. |
| `crates/engine/src/database/tests/open.rs` | engine inline tests | Database open behavior | Required | Rewrite | `M5T` | `M5TA` | none | Product open survives. | Delete follower open cases; use V1 cache/local/IPC open policy. |
| `crates/engine/src/database/tests/regressions.rs` | engine inline tests | Mixed recovery/checkpoint/follower regressions | Redesign | Rewrite | `M4T` | `M4TD` | none | Many recovery cases survive; follower cases do not. | Split file; delete follower-specific cases. |
| `crates/engine/src/database/tests/search_branch_cleanup.rs` | engine inline tests | Search cleanup on branch operations | Required | Rewrite | `M6T` | `M6TE` | none | Derived-state cleanup survives. | Align with retrieval/derived-state contract. |
| `crates/engine/src/database/tests/shutdown.rs` | engine inline tests | Shutdown behavior | Required | Rewrite | `M5T` | `M5TA` | none | Lifecycle behavior survives. | Delete follower shutdown cases; assert structured errors. |
| `crates/engine/src/database/tests/snapshot_retention.rs` | engine inline tests | Snapshot retention behavior | Required | Rewrite | `M4T` | `M4TC` | none | Retention survives under storage-next. | Remove public maintenance assumptions. |
| `crates/engine/src/database/tests/transactions.rs` | engine inline tests | Database transaction behavior | Redesign | Rewrite | `M6T` | `M6TA` | none | Public transaction sessions removed; atomic commits remain. | Split surviving internal assertions. |
| `crates/engine/tests/adversarial_tests.rs` | engine integration | Engine adversarial behavior | Required | Rewrite | `M10T` | `M10TB` | none | High-value hardening. | Port late. |
| `crates/engine/tests/architecture_doc_truth.rs` | engine docs guard | Architecture doc truth guard | Redesign | Rewrite | `M10T` | `M10TE` | docs | Doc truth remains, doc set changes. | Supports the M10E2 docs/examples audit after V1 docs stabilize. |
| `crates/engine/tests/branch_id_characterization.rs` | engine integration | BranchId behavior | Required | Rewrite | `M1T` | `M1TA` | none | BranchId moves to core-next. | Port to core atom tests. |
| `crates/engine/tests/branch_isolation_tests.rs` | engine integration | Branch isolation | Required | Rewrite | `M6T` | `M6TC` | none | Product behavior survives. | Use V1 branch API. |
| `crates/engine/tests/concurrency_tests.rs` | engine integration | Concurrency behavior | Required | Rewrite | `M10T` | `M10TA` | none | Concurrency remains critical. | Prefer deterministic scheduler. |
| `crates/engine/tests/config_matrix.rs` | engine integration | Configuration matrix | Required | Rewrite | `M5T` | `M5TD` | none | Runtime profiles/config survive. | Align with resource profile contract. |
| `crates/engine/tests/crash_simulation_test.rs` | engine integration | Crash simulation | Required | Rewrite | `M4T` | `M4TD` | none | Crash recovery survives. | Move storage crash pieces to M4. |
| `crates/engine/tests/cross_primitive_tests.rs` | engine integration | Cross-primitive behavior | Required | Rewrite | `M6T` | `M6TA` | none | Capability composition survives. | Recast as KV-backed capabilities. |
| `crates/engine/tests/data/porter_output.txt` | engine fixture | Porter stemmer expected output | Required | Keep | `M8T` | `M8TD` | none | Search/tokenization fixture remains useful. | Move if retrieval owns tokenizer tests. |
| `crates/engine/tests/data/porter_voc.txt` | engine fixture | Porter stemmer vocabulary | Required | Keep | `M8T` | `M8TD` | none | Search/tokenization fixture remains useful. | Move if retrieval owns tokenizer tests. |
| `crates/engine/tests/database_open_test.rs` | engine integration | Database open behavior | Required | Rewrite | `M5T` | `M5TA` | none | Product open survives. | Align cache/local/IPC behavior. |
| `crates/engine/tests/database_transaction_tests.rs` | engine integration | Public transaction APIs | Redesign | Rewrite | `M6T` | `M6TA` | none | Manual sessions removed; atomic write intent survives. | Delete begin/commit session cases. |
| `crates/engine/tests/flush_pipeline_tests.rs` | engine integration | Flush pipeline behavior | Redesign | Rewrite | `M4T` | `M4TD` | none | Internal flushing remains; user command removed. | Move to storage lifecycle tests. |
| `crates/engine/tests/follower_tests.rs` | engine integration | Follower mode | Remove | Delete | none | none | none | Follower mode is removed. | IPC covers same-machine sharing. |
| `crates/engine/tests/m4_pooling_tests.rs` | engine integration | Old milestone pooling behavior | Evidence-only | Archive | none | none | none | Milestone-shaped old pooling test should not constrain V1. | Reuse only if storage commit pool survives. |
| `crates/engine/tests/memory_profiling.rs` | engine integration | Memory profile behavior | Required | Rewrite | `M5T` | `M5TD` | none | Runtime resource profiles are V1 required. | Update thresholds/policy. |
| `crates/engine/tests/primitives_cross_tests.rs` | engine integration | Cross-primitive capability behavior | Required | Rewrite | `M6T` | `M6TA` | none | Behavior survives. | Use V1 capability contract. |
| `crates/engine/tests/recovery_parity.rs` | engine integration | Recovery parity | Required | Rewrite | `M4T` | `M4TD` | none | Recovery parity survives. | Delete follower leg; split storage vs engine assertions. |
| `crates/engine/tests/recovery_storage_policy.rs` | engine integration | Recovery/storage policy | Required | Rewrite | `M5T` | `M5TB` | none | Engine-storage policy boundary survives. | Remove follower-derived expectations; use storage-next error mapping. |
| `crates/engine/tests/recovery_tests.rs` | engine integration | Recovery behavior | Required | Rewrite | `M4T` | `M4TD` | none | Recovery survives. | Port to storage/engine split. |
| `crates/engine/tests/robustness_regressions.rs` | engine integration | Robustness regressions | Required | Rewrite | `M10T` | `M10TB` | none | High-value hardening. | Port after V1 surfaces settle. |
| `crates/engine/tests/surface_regression.rs` | engine integration | Public surface regression | Required | Rewrite | `M5T` | `M5TA` | none | Surface guard survives. | Update to V1 public API. |
| `crates/engine/tests/versioned_conformance_tests.rs` | engine integration | Versioned read/history behavior | Required | Rewrite | `M6T` | `M6TD` | none | Version/time-travel behavior survives. | Align getv/history/as_of. |
| `crates/intelligence/tests/embed_model_lifecycle_tests.rs` | intelligence crate | Embedding model lifecycle | Required | Rewrite | `M8T` | `M8TB` | none | Autoembedding lifecycle survives. | Add model mismatch behavior. |
| `crates/intelligence/tests/embed_pipeline_tests.rs` | intelligence crate | Embedding pipeline | Required | Rewrite | `M8T` | `M8TB` | none | Pipeline survives. | Use engine surface consumed contract. |
| `crates/intelligence/tests/expand_cache_fork_test.rs` | intelligence crate | Expansion cache branch/fork behavior | Required | Rewrite | `M8T` | `M8TD` | none | Expansion cache survives. | Align with derived-state freshness. |
| `crates/intelligence/tests/generate_lifecycle_tests.rs` | intelligence crate | Generation lifecycle | Required | Rewrite | `M8T` | `M8TA` | none | Generation orchestration survives. | Inference owns provider details. |
| `crates/storage/src/segmented/tests/basic.rs` | storage inline tests | Segmented-store basics | Required | Rewrite | `M4T` | `M4TA` | none | Table/branch runtime behavior survives. | Rename around storage-next table runtime. |
| `crates/storage/src/segmented/tests/batch.rs` | storage inline tests | Batch write behavior | Required | Rewrite | `M4T` | `M4TC` | none | Atomic commit batches survive internally. | Align with public batch semantics later. |
| `crates/storage/src/segmented/tests/compact.rs` | storage inline tests | Compaction behavior | Required | Rewrite | `M4T` | `M4TC` | none | Compaction remains internal maintenance. | No user maintenance command assumptions. |
| `crates/storage/src/segmented/tests/concurrency.rs` | storage inline tests | Segmented-store concurrency | Required | Rewrite | `M4T` | `M4TC` | none | Concurrency survives. | Use deterministic scheduler where possible. |
| `crates/storage/src/segmented/tests/flush.rs` | storage inline tests | Flush behavior | Required | Rewrite | `M4T` | `M4TD` | none | Internal flush survives. | Cache mode has no durable flush. |
| `crates/storage/src/segmented/tests/fork.rs` | storage inline tests | Branch fork behavior | Required | Rewrite | `M4T` | `M4TB` | none | Branch-isolated storage survives. | Align with branch-aware LSM. |
| `crates/storage/src/segmented/tests/gc_under_degradation.rs` | storage inline tests | GC under degraded state | Required | Rewrite | `M4T` | `M4TD` | none | Recovery/degradation survives. | Assert V1 health classes. |
| `crates/storage/src/segmented/tests/leveled.rs` | storage inline tests | Leveled storage behavior | Required | Rewrite | `M4T` | `M4TA` | none | Table runtime survives. | Use V1 table terminology. |
| `crates/storage/src/segmented/tests/lifecycle.rs` | storage inline tests | Segmented-store lifecycle | Required | Rewrite | `M4T` | `M4TE` | none | Lifecycle survives. | Align cache/standard/always. |
| `crates/storage/src/segmented/tests/materialize.rs` | storage inline tests | Branch materialization and inherited layers | Required | Rewrite | `M4T` | `M4TB` | none | Branch-aware storage behavior survives. | Split oversized file into focused tests. |
| `crates/storage/src/segmented/tests/mod.rs` | storage inline harness | Segmented tests harness | Redesign | Rewrite | `M4T` | `M4TA` | segmented modules | Harness must match storage-next modules. | Rename away from old segmented shape if needed. |
| `crates/storage/src/segmented/tests/post_restart_branch.rs` | storage inline tests | Post-restart branch behavior | Required | Rewrite | `M4T` | `M4TD` | none | Recovery and branch visibility survive. | Use L9 recovery API. |
| `crates/storage/src/segmented/tests/publish_failures.rs` | storage inline tests | Durable publish failure behavior | Required | Rewrite | `M3T` | `M3TC` | none | Fault windows survive. | Move to L4 durable publisher tests. |
| `crates/storage/src/segmented/tests/quarantine_reconciliation.rs` | storage inline tests | Quarantine reconciliation | Required | Rewrite | `M4T` | `M4TD` | none | Quarantine/recovery survives. | Align with L8 health output. |
| `crates/storage/src/segmented/tests/resurrection.rs` | storage inline tests | Tombstone/resurrection behavior | Required | Rewrite | `M4T` | `M4TC` | none | Tombstone/retention behavior survives. | Include TTL/retention matrix. |

## Benchmarks And Regression Corpora

| path | current_area | behavior | v1_decision | action | target_track | target_epic | fixtures | reason | notes |
|---|---|---|---|---|---|---|---|---|---|
| `benchmarks/Cargo.lock` | benchmark crate | Benchmark dependency lockfile | Redesign | Rewrite | `M10T` | `M10TD` | none | Benchmark crate dependencies may change. | Refresh with V1 benchmark crate. |
| `benchmarks/Cargo.toml` | benchmark crate | Benchmark crate manifest | Required | Rewrite | `M10T` | `M10TD` | none | Benchmark suite remains binding. | Update dependencies to V1 crates. |
| `benchmarks/baselines/.gitkeep` | benchmark baseline support | Baseline directory placeholder | Evidence-only | Keep | `M10T` | `M10TD` | none | Directory remains useful. | Support-only file. |
| `benchmarks/baselines/b6-baseline.json` | benchmark baseline | Historical branch baseline | Evidence-only | Archive | none | none | none | Pre-V1 baseline is evidence only. | V1 thresholds must be re-derived. |
| `benchmarks/baselines/pre-branch-cleanup.json` | benchmark baseline | Historical branch cleanup baseline | Evidence-only | Archive | none | none | none | Pre-V1 baseline is evidence only. | V1 thresholds must be re-derived. |
| `benchmarks/baselines/t1-complete.json` | benchmark baseline | Historical tranche baseline | Evidence-only | Archive | none | none | none | Pre-V1 baseline is evidence only. | V1 thresholds must be re-derived. |
| `benchmarks/benches/redb_benchmark.rs` | benchmark | redb-style comparison benchmark | Required | Rewrite | `M10T` | `M10TD` | none | Competitive benchmark remains binding. | Update to V1 public API. |
| `benchmarks/benches/redb_common.rs` | benchmark support | Shared embedded-db comparison helpers | Required | Rewrite | `M10T` | `M10TD` | none | Benchmark helper remains useful. | Update setup and durability knobs. |
| `benchmarks/benches/wal_latency.rs` | benchmark | WAL latency benchmark | Required | Rewrite | `M10T` | `M10TD` | none | Durable-mode latency remains important. | Move WAL specifics behind storage-next metrics. |
| `benchmarks/benches/ycsb_compare.rs` | benchmark | YCSB comparison benchmark | Required | Rewrite | `M10T` | `M10TD` | none | YCSB regression gate remains useful. | Update to V1 write/read APIs. |
| `benchmarks/benches/ycsb_workloads.rs` | benchmark support | YCSB workload definitions | Required | Keep | `M10T` | `M10TD` | none | Workload definitions remain reusable. | Review only if API assumptions leak in. |
| `benchmarks/history/.gitkeep` | benchmark history support | History directory placeholder | Evidence-only | Keep | `M10T` | `M10TD` | none | Directory remains useful. | Support-only file. |
| `benchmarks/history/history.json` | benchmark history | Historical benchmark run history | Evidence-only | Archive | none | none | none | Historical trend data is not a V1 gate. | Keep as evidence; V1 starts new history. |
| `benchmarks/results/wal-latency-2026-04-15T16-41-06Z-e33da45d.json` | benchmark result | Historical WAL latency result | Evidence-only | Archive | none | none | none | Pre-V1 result is evidence only. | V1 latency thresholds must be re-derived. |
| `benchmarks/results/wal-latency-2026-04-17T19-10-05Z-528760b5.json` | benchmark result | Historical WAL latency result | Evidence-only | Archive | none | none | none | Pre-V1 result is evidence only. | V1 latency thresholds must be re-derived. |
| `benchmarks/results/wal-latency-2026-04-17T19-11-26Z-9fd47985.json` | benchmark result | Historical WAL latency result | Evidence-only | Archive | none | none | none | Pre-V1 result is evidence only. | V1 latency thresholds must be re-derived. |
| `benchmarks/src/bin/beir.rs` | benchmark binary | BEIR retrieval benchmark | Optional | Rewrite | `M10T` | `M10TD` | datasets external | Retrieval benchmark remains useful. | Optional model-dependent gate. |
| `benchmarks/src/bin/compare.rs` | benchmark binary | Benchmark report comparison tool | Required | Rewrite | `M10T` | `M10TD` | benchmark reports | Comparison tooling remains useful. | Update schema if needed. |
| `benchmarks/src/bin/regression.rs` | benchmark binary | Regression benchmark runner | Required | Rewrite | `M10T` | `M10TD` | baselines/history/results | Release gate runner remains useful. | Remove old tranche/epic assumptions. |
| `benchmarks/src/harness/metrics.rs` | benchmark harness | Process metrics capture | Required | Keep | `M10T` | `M10TD` | none | Runtime profile benchmarking needs metrics. | Review platform fallback. |
| `benchmarks/src/harness/mod.rs` | benchmark harness | Shared benchmark database/data helpers | Required | Rewrite | `M10T` | `M10TD` | none | Harness survives but public API changes. | Update Strata open/config APIs. |
| `benchmarks/src/harness/recorder.rs` | benchmark harness | Benchmark result recorder | Required | Rewrite | `M10T` | `M10TD` | results/history | Recorder survives. | Ensure V1 metadata fields match NFRs. |
| `benchmarks/src/harness/scaling.rs` | benchmark harness | Multi-threaded scaling experiments | Required | Rewrite | `M10T` | `M10TD` | none | Scaling coverage remains useful. | Update write APIs and resource profiles. |
| `benchmarks/src/lib.rs` | benchmark crate | Benchmark crate public modules | Required | Rewrite | `M10T` | `M10TD` | harness/schema | Crate shape may change. | Keep benchmark-only surface. |
| `benchmarks/src/schema.rs` | benchmark harness | Benchmark result schema | Required | Rewrite | `M10T` | `M10TD` | reports | Schema remains useful. | Add V1 profile/backend metadata if needed. |
| `crates/engine/benches/primitive_benchmarks.rs` | benchmarks | Primitive performance | Required | Rewrite | `M10T` | `M10TD` | none | Performance gate survives. | Update to V1 capability APIs. |
| `crates/engine/benches/transaction_benchmarks.rs` | benchmarks | Transaction performance | Redesign | Rewrite | `M10T` | `M10TD` | none | Public transactions removed. | Recast as write/batch/commit benchmarks. |
| `crates/engine/benches/vector_benchmarks.rs` | benchmarks | Vector performance | Required | Rewrite | `M10T` | `M10TD` | none | Vector performance remains required. | Include shadow-vector/retrieval paths if needed. |
| `crates/storage/proptest-regressions/key_encoding.txt` | proptest corpus | Storage key encoding regression seed | Required | Keep | `M3T` | `M3TA` | none | Keep if key-encoding property tests survive. | Refresh if storage key format changes. |

## M0TE Closure

M0TE is closed when:

1. every file in the inventory scope has a row above;
2. every row has a V1 decision and action;
3. every executable or fixture `Keep` or `Rewrite` row has a target track;
4. every `Delete` or `Archive` row explains the removed or historical behavior;
5. later milestones can filter this document by `target_track` before porting
   tests.
