# L8U Implementation Plan: Durable Rewrite Publication

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8u-durable-rewrite-publication-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8s-table-object-reachability-retention-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8t-table-manifest-backed-flush-watermarks-implementation-plan.md`

## Objective

Publish compaction and materialization rewrite outputs durably.

L8K added conservative scheduling: durable-local compaction and
materialization could run through L6 but still reported checkpoint debt because
table-manifest recovery did not exist. L8Q-L8T add the missing durable table
foundation. L8U upgrades rewrite handling so compacted/materialized outputs can
be published as local durable table objects, installed into L6, recorded in
branch table manifests, and recovered without waiting for a row-native
checkpoint.

The slice must preserve lower-layer ownership:

1. L5 owns table bytes and table compaction behavior.
2. L6 owns candidate validation, branch rewrite semantics, materialization
   handles, child-local precedence, and atomic branch-state install.
3. L4 owns durable table-object and table-manifest publication.
4. L8 owns operation order, fault-window outcomes, health debt, and durable
   proof wiring.

L8U must not add row pruning. It publishes keep-all rewrite outputs unless a
later L8V retention proof explicitly allows dropping old versions, tombstones,
or TTL-expired rows.

## Inputs

1. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
2. `docs/architecture/storage/l5-table-runtime.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
5. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
6. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L8/l8s-table-object-reachability-retention-implementation-plan.md`
10. `docs/architecture/implementation-plans/M4/L8/l8t-table-manifest-backed-flush-watermarks-implementation-plan.md`
11. `crates/storage-next/src/lifecycle/compaction.rs`
12. `crates/storage-next/src/lifecycle/flush.rs`
13. `crates/storage-next/src/lifecycle/recovery.rs`
14. `crates/storage-next/src/lifecycle/durable/maintenance.rs`
15. `crates/storage-next/src/lifecycle/maintenance.rs`
16. `crates/storage-next/src/service/table.rs`
17. `crates/storage-next/src/service/manifest.rs`
18. `crates/storage-next/src/branch/state.rs`
19. `crates/storage-next/src/table/compaction.rs`
20. `crates/storage/src/segmented/compaction.rs`
21. `crates/storage/src/segmented/mod.rs`
22. `crates/storage/src/segmented/tests/publish_failures.rs`
23. `crates/storage/src/segmented/tests/resurrection.rs`

## Existing-Code Source Map

| Current file | Evidence | L8U action |
|---|---|---|
| `lifecycle/compaction.rs` | L8K already routes compaction/materialization through L6 and reports checkpoint debt. | Replace checkpoint-only durable behavior with publish-before-install table-object and manifest flows. Keep volatile/cache behavior unchanged. |
| `lifecycle/flush.rs` | Flush already publishes table object, reopens/validates it, installs into L6, then reports partial-progress facts. | Reuse the same publication discipline for rewrite outputs. Do not duplicate validation shortcuts. |
| `service/table.rs` | `TableObjectService::publish_create` validates table bytes and `TableObjectReaderService::open_reader` validates object facts. | Every rewrite output table object must be published and reopened before L6 install. |
| `service/manifest.rs` | L8R typed table-manifest service publishes branch table manifests. | Publish updated branch table manifest after L6 rewrite install succeeds. |
| `branch/state.rs` | L6 owns compaction plans, materialization handles, stale-candidate checks, output identity validation, and atomic install. | L8U must use L6 plan/install APIs. It must not mutate `owned_levels` or inherited layers directly. |
| `table/compaction.rs` | L5 compactor owns row merge and output splitting. | L8U may receive output table bytes/facts from L5/L6 but must not alter merge logic. |
| `lifecycle/retention.rs` | L8S can classify replaced table objects as live/retained/quarantine candidates. | Replaced input objects are retained until table manifests and L8S proof allow later quarantine. No deletion here. |
| `lifecycle/checkpoint.rs` | L8T may use table-manifest coverage to shorten WAL. | L8U can reduce checkpoint debt by publishing manifests, but flush watermark advancement remains L8T. |

## Old Codebase Porting Map

The old storage engine published replacement segments and manifests around
compaction/materialization. L8U ports the ordering and failure-window
discipline, not the path-based implementation.

| Old source | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `segmented/compaction.rs::compact_l0_to_l1` | Builds replacement table files, validates overlap, installs atomically, and writes manifest. | L6 owns candidate/output validation. L8U publishes output table objects before install and table manifest after install. | Publish-before-install and manifest-after-install order. |
| `segmented/compaction.rs::compact_level` | Replaces input tables with output tables without changing visible reads. | Preserve read parity through L6/L5 keep-all compaction. | Latest/history/range/tombstone parity after durable compaction. |
| `segmented/mod.rs::materialize_layer` | Materialization preserves child-local precedence and removes inherited layer only after replacement is visible. | Use L6 materialization handle and install path. Publish replacement table objects before L6 materialization install. | Child-local precedence, fork gate, retry, and handle-bound materialization. |
| `segmented/tests/publish_failures.rs` | Publish failure before manifest leaves old state visible; manifest/fsync failure after install is forward-progress debt. | Translate to table-object publish, reopen, L6 install, and table-manifest publish fault windows. | Typed outcomes for each window, old reads preserved where required. |
| `segmented/tests/resurrection.rs` | Stale compaction/materialization must not resurrect cleared/deleted branch state after I/O. | Preserve with stale candidate and manifest epoch validation before install. | Stale candidate fails closed after output publication, with objects retained for L8S. |
| `test_hooks.rs` | Pause hooks split I/O from install for race tests. | Use deterministic fake services/fault hooks instead of global pause hooks. | Fault seams are local to tests and do not add production test hooks. |
| `gc_orphan_segments` | Published but unmanifested objects are orphans for later safe reclaim. | L8U reports orphan/published-not-installed facts and leaves retention to L8S/L8M. | Orphaned output object is named and not deleted. |

Do not port:

1. raw filesystem paths, renames, or directory fsyncs into lifecycle;
2. direct manifest writes from compaction handlers;
3. public compaction commands or product wording;
4. row pruning without L8V proof;
5. direct deletion/quarantine/purge of replaced table objects;
6. background compaction threads;
7. old global pause hooks;
8. primitive/query/vector/graph vocabulary.

## Scope

L8U implements:

1. durable rewrite publication request and outcome types;
2. durable compaction output publication through L4 table object service;
3. durable materialization replacement publication through L4 table object
   service;
4. validation/reopen of every output table object before L6 install;
5. L6 atomic install/swap after output validation;
6. branch table-manifest publication after L6 install succeeds;
7. table-manifest recovery facts that let L8T use rewrite outputs as coverage;
8. typed outcomes for:
   - no candidate;
   - publish failed before install;
   - reopen failed before install;
   - install failed after publish;
   - manifest publish failed after install;
   - manifest publication uncertain;
   - completed durable rewrite;
9. affected object names for output, replaced, retained, and orphaned objects;
10. health debt for every ambiguous or partial-progress window;
11. generated/testkit counters for compaction and materialization rewrite
    publication;
12. source guards preventing row pruning, raw IO, direct deletion, and product
    imports.

L8U does not implement:

1. new compaction algorithms;
2. row-version, tombstone, or TTL pruning;
3. table-object retention/quarantine/purge;
4. WAL truncation or flush-watermark persistence;
5. lazy object-backed readers;
6. memory-budget admission;
7. branch delete/clear/generation completion;
8. public storage API exposure.

## Publication Protocol

Target durable rewrite sequence:

```text
require durable local open runtime
acquire ordinary maintenance admission
ask L6 for compaction/materialization candidate and plan
build output table bytes through L5/L6
for each output:
  publish table object through L4
  reopen and validate object facts
preflight L6 install with validated outputs and current candidate epoch
install branch rewrite atomically through L6
rebuild branch table manifest from current L6 reachability and durable catalog
publish branch table manifest through L4
return durable rewrite outcome
```

Rules:

1. Output table objects are published before L6 install.
2. Output table objects are reopened and validated before L6 install.
3. L6 install is all-or-nothing at the branch-state boundary.
4. Branch table manifest is published after L6 install.
5. Manifest publish failure after install is forward-progress debt, not rollback.
6. Published output objects from failed install are not deleted. They become
   L8S orphan/quarantine candidates once proof is safe.
7. Replaced input objects are not deleted. They remain retained until L8S/L8M
   prove safe reclaim.
8. Table-manifest publication success does not directly truncate WAL. L8T owns
   watermark proof and WAL deletion.

## Compaction Semantics

Compaction durability must preserve L6/L5 semantics.

Rules:

1. L6 selects candidate inputs and output level.
2. L5 builds output table bytes and reports compaction facts.
3. L8U publishes the exact L5/L6 outputs; it does not inspect rows to modify
   merge behavior.
4. Keep-all policy remains default until L8V.
5. Tombstones, older versions, TTL-expired rows, timestamps, and branch ids are
   preserved unless L8V supplies a proof-gated pruning policy.
6. Same input candidate with same output identity seed must produce retryable
   deterministic object names unless the previous output facts conflict.
7. Candidate stale after I/O fails closed before L6 install.

## Materialization Semantics

Materialization durability must preserve L6 handle semantics.

Rules:

1. Lifecycle must bind materialization intent to a L6 handle before output I/O.
2. A queued materialization task must not target a naked layer index after
   earlier layer removal can reindex the vector.
3. Replacement output table objects are published before inherited-layer removal.
4. Child-owned rows keep precedence over materialized inherited rows.
5. Existing partial replacement output objects are reused only when identity,
   object, facts, and provenance match.
6. Retry after layer removal must use source identity/fork facts, not vector
   index.
7. Manifest publish failure after materialization install leaves recovery able
   to use the previous manifest or WAL/checkpoint path and records health debt.

## Durable Catalog And Manifest Updates

L8U consumes the durable table catalog introduced by L8R.

Rules:

1. Add output table entries only after publish and reopen validation.
2. Mark replaced input refs as retained until L8S classifies them.
3. Manifest update must represent current L6 reachability after install.
4. Manifest update must not include published-but-not-installed output refs.
5. Manifest update must preserve unrelated branch table refs and inherited
   layers.
6. Catalog ambiguity blocks manifest publication.
7. Recovered catalog facts must be enough to retry failed manifest publication.

## Error And Health Vocabulary

Add typed lifecycle errors/faults for:

1. rewrite output publish failed;
2. rewrite output publish uncertain;
3. rewrite output reopen failed;
4. rewrite output fact mismatch;
5. rewrite install failed after publish;
6. rewrite manifest publish failed;
7. rewrite manifest publish uncertain;
8. rewrite stale candidate after publish;
9. rewrite orphan output recorded;
10. rewrite replaced object retained for proof;
11. materialization handle stale;
12. materialization source mismatch.

Every error must expose a stable code and preserve source chains.

## Source Boundaries

L8U may import:

1. L6 compaction/materialization plan/install APIs;
2. L5 table compaction output APIs through L6/L5 public crate-private surfaces;
3. L4 table object and table manifest services;
4. L8R durable table catalog types;
5. L8S retention decision types for outcome facts only;
6. L8T coverage facts for checkpoint-debt reduction only.

L8U must not import:

1. raw filesystem APIs;
2. backend delete APIs;
3. quarantine mutation or purge APIs;
4. row-pruning policy code;
5. engine/product crates;
6. StrataHub code;
7. primitive DTOs;
8. query/index/autosearch modules.

## Implementation Steps

1. Extend lifecycle compaction/materialization durability enum with durable
   table-manifest-backed publication.
2. Add output publication helpers shared by compaction and materialization.
3. Publish and reopen output table objects before install.
4. Add stale-candidate validation after publication and before install.
5. Install through L6.
6. Publish branch table manifest through L8R service/catalog.
7. Convert partial-progress windows into typed outcomes and health debt.
8. Add generated counters, direct tests, source guards, and porting-log entry.

## Deferred Behavior

Deferred to L8V:

1. dropping older versions;
2. tombstone cleanup;
3. TTL cleanup;
4. retention-aware compaction policies.

Deferred to L8S/L8M:

1. quarantine of replaced input objects;
2. quarantine of published-but-not-installed output objects;
3. purge and repair.

Deferred to L8W:

1. memory-budget admission for large rewrite outputs;
2. output build throttling by memory profile.

Deferred to L8X:

1. lazy recovery/open of large rewrite outputs;
2. block-cache integration for rewrite outputs.

## Exit Gate

L8U is complete when:

1. durable compaction publishes and validates output table objects before L6
   install;
2. durable materialization publishes and validates replacement table objects
   before inherited-layer removal;
3. branch table manifest is published after successful rewrite install;
4. every publish/reopen/install/manifest failure window has typed outcome facts;
5. old input objects and orphan output objects are retained for L8S/L8M proof;
6. keep-all read parity is preserved;
7. table-manifest coverage facts are available for L8T;
8. cache mode still runs volatile rewrites without durable claims;
9. source guards block raw IO, deletion, product imports, and row pruning;
10. generated and direct tests cover compaction and materialization paths.
