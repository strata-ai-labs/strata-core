# L8P Implementation Plan: Lifecycle Conformance Closeout

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8p-lifecycle-conformance-closeout-test-plan.md`

## Objective

Close the M4-L8 lifecycle, recovery, maintenance, reclaim, close, and assurance
milestone.

L8P does not add a new lifecycle feature. It proves that the behavior built in
L8A through L8O is internally consistent, source-boundary clean, covered by
direct/generated/fault/crash/fuzz assurance, and ready for L9 to wrap without
importing product lifecycle policy into storage.

L8P may fix bugs discovered during closeout, but the owning code should remain
in the earlier lifecycle module that owns the behavior. The closeout slice
should primarily add:

1. implementation inventory;
2. source-boundary and placement guards;
3. generated/fault/crash/fuzz coverage inventory;
4. sensitivity-probe ledger;
5. command-matrix evidence;
6. explicit deferral map for work owned by L9, engine-next, or post-V1;
7. final porting-log closeout record.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/storage/l7-commit-runtime.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/storage/l5-table-runtime.md`
5. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
6. `docs/architecture/storage/implementation-patterns.md`
7. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
8. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
9. All L8A through L8O slice plans under
   `docs/architecture/implementation-plans/M4/L8/`
10. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md`
11. `crates/storage-next/src/lifecycle/`
12. `crates/storage-next/src/lifecycle/tests/`
13. `crates/storage-next/src/testkit/lifecycle/`
14. `crates/storage-next/tests/lifecycle_*.rs`
15. `crates/storage-next/tests/crash_recovery.rs`
16. `crates/storage-next/fuzz/Cargo.toml`
17. `crates/storage-next/fuzz/fuzz_targets/lifecycle_*.rs`
18. `crates/storage-next/fuzz/corpus/lifecycle_*`

## Old-Code Source Map

L8P should verify that the storage-next runtime preserved storage mechanics from
the old architecture without porting product-level responsibilities.

| Old source | Closeout evidence to verify | Storage-next closeout check |
|---|---|---|
| `crates/engine/src/database/open.rs` | Open/create orders capability checks before durable side effects and exposes raw storage facts upward. | Inventory tests and source guards prove cache/durable open paths and forbid product open policy. |
| `crates/engine/src/database/recovery.rs` | Recovery classifies corrupt/missing storage without product wording. | Recovery/fault inventory proves typed health classes and source-chain preservation. |
| `crates/engine/src/database/lifecycle.rs` | Close gates new work, drains/cancels background work, syncs durable state, and releases ownership in order. | Closeout inventory points to close tests and generated close counters. |
| `crates/engine/src/background.rs` | Maintenance queue ordering, cancel, and drain behavior are deterministic. | Maintenance generated/source tests prove deterministic executor behavior without sleeps or threads. |
| `crates/storage/src/durability/recovery_bootstrap.rs` | Recovered durable facts bootstrap visible version and commit clocks. | Bootstrap inventory proves allocator, timestamp guard, visible version, and unresolved-gate reconciliation coverage. |
| `crates/storage/src/durability/recovery.rs` | Manifest/snapshot/WAL/table recovery uses strict and lossy classification. | Fault and generated recovery evidence prove healthy/degraded/failed separation. |
| `crates/storage/src/durability/checkpoint_runtime.rs` | Checkpoint publication, manifest update, and pruning have durable ordering. | Checkpoint/WAL truncation closeout checks prove publication windows and retention proof gates. |
| `crates/storage/src/durability/compaction/wal_only.rs` | WAL truncation is proof-driven. | Closeout verifies no truncation path runs without typed checkpoint/flush proof coverage. |
| `crates/storage/src/segmented/compaction.rs` | Compaction/materialization are scheduled around branch reachability. | Table rewrite inventory proves L8 schedules and reports, while L5/L6 own algorithms. |
| `crates/storage/src/segmented/quarantine_protocol.rs` | Quarantine precedes purge and repair is fact-reporting. | Reclaim/quarantine/purge/repair coverage and sensitivity probes verify safety. |
| `crates/storage/src/segmented/ref_registry.rs` | Reachability drives object deletion. | Retention closeout verifies L6 reachability facts remain the deletion authority. |

L8P must not port:

1. public database open/close APIs;
2. engine primitive freeze/rebuild hooks;
3. IPC, follower, or multi-process product behavior;
4. product background worker threads;
5. public manual maintenance commands;
6. user-facing recovery advice;
7. StrataHub sync or cloud behavior;
8. query/index/search side effects.

## Current State

L8A through L8O have built:

1. lifecycle vocabulary, errors, outcomes, health, and stats;
2. lifecycle state machine and operation admission;
3. storage mode and backend capability validation;
4. cache-mode open, commit, maintenance, and close baseline;
5. durable local service assembly;
6. recovery orchestration over manifest, snapshot, WAL, table objects, and
   quarantine inventory;
7. L7 replay/bootstrap, allocator catch-up, timestamp guard catch-up, visible
   catch-up, and unresolved durable gate reconciliation;
8. deterministic maintenance executor;
9. flush to table objects;
10. checkpoint, flush watermark, and WAL truncation;
11. compaction and materialization scheduling;
12. retention proof and snapshot pruning;
13. quarantine, purge, repair, and reclaim orchestration;
14. close ordering, maintenance drain/cancel, quiesce, durable sync, and writer
   guard release;
15. generated script, fault, crash/reopen, and fuzz assurance.

L8P should treat that surface as the subject under test. If closeout discovers a
behavior bug, fix the behavior in the owning module and record the fix in the
porting log.

## Scope

L8P implements:

1. `crates/storage-next/tests/lifecycle_closeout.rs`;
2. any missing implementation-focused checks in
   `crates/storage-next/tests/lifecycle_source_guard.rs`;
3. any missing fuzz/corpus checks in
   `crates/storage-next/tests/lifecycle_fuzz_inventory.rs`;
4. final closeout section in `m4-l8-porting-log.md`;
5. final sensitivity-probe ledger for lifecycle-specific mutations;
6. final command-matrix evidence;
7. explicit deferral map for non-L8 work.

L8P does not implement:

1. public L9 lifecycle APIs;
2. product open/close/recovery wording;
3. engine observer callbacks;
4. background worker threads;
5. process-kill matrix in default CI;
6. distributed object-store fencing or lease races;
7. StrataHub behavior;
8. documentation-only tests that merely assert plan documents exist or contain
   links.

## Closeout Principles

1. Automated closeout tests inspect implementation artifacts, source boundaries,
   and runnable test/fuzz assets.
2. Automated closeout tests do not prove correctness by checking planning
   document structure.
3. Source guards reject real boundary drift: product imports, raw IO imports,
   public API leakage, testkit/fuzz leakage, and lower-layer upward imports.
4. Fuzz inventory proves target registration, distinct routing, and named
   checked-in seed corpora.
5. Generated/fault/crash inventory proves category counters are asserted by
   tests, not merely produced by testkit helpers.
6. Sensitivity probes are recorded as evidence. Mutation edits must not remain
   in the committed tree.
7. Deferrals must name owner and rationale.

## File Layout

Preferred additions:

```text
crates/storage-next/tests/lifecycle_closeout.rs
```

Possible updates:

```text
crates/storage-next/tests/lifecycle_source_guard.rs
crates/storage-next/tests/lifecycle_fuzz_inventory.rs
docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md
```

Do not add tests whose only assertion is that this plan, the parent plan, or
slice plans exist. Documentation consistency is reviewed in the porting log and
by human review; automated tests stay implementation-focused.

## Implementation Steps

### L8P-A: Inventory The Landed Runtime

Read the L8 runtime code and tests and record a final closeout inventory in the
porting log:

1. production lifecycle modules;
2. direct module-local tests;
3. integration tests;
4. generated property tests;
5. fault-window tests;
6. crash/reopen tests;
7. source guards;
8. fuzz targets and corpora;
9. feature/no-default/wasm/fuzz build surfaces;
10. explicit deferrals.

Confirm that lifecycle remains crate-private and that L9 is still the future
public boundary.

### L8P-B: Add Closeout Inventory Tests

Create `lifecycle_closeout.rs` with implementation-focused checks.

Required checks:

1. generated script/property tests assert input-derived open/recovery,
   maintenance, reclaim, validation, visibility, deletion, watermark, close,
   cache-mode, and degraded-health counters;
2. integration tests assert generated, fault, and crash category counters;
3. fault tests cover capability, writer guard, manifest, snapshot, WAL tail,
   corrupt WAL, replay, visible publication, flush orphan, table rewrite,
   retention, quarantine, purge, and close windows;
4. crash tests cover WAL append, unresolved gate, snapshot orphan, checkpoint
   tail, table orphan, quarantine inventory, quarantine-before-purge, and close
   reopen windows;
5. fuzz inventory covers recovery, maintenance, and retention targets, distinct
   contract functions, and exact named seed corpora;
6. source guards cover production/testkit/fuzz separation, no sleeps/threads,
   crash gating, source-boundary imports, and architecture-label avoidance in
   Rust implementation/test source;
7. no lifecycle mutation-probe scratch file is present in the repository.

### L8P-C: Consolidate Source Guards

Review `lifecycle_source_guard.rs` against the parent source-guard policy.
Extend it only for real boundary gaps.

It must reject:

1. production lifecycle imports from engine, product, StrataHub, or public API
   modules;
2. raw filesystem/env/process-global IO in lifecycle production source;
3. lifecycle imports from lower storage layers;
4. public lifecycle module exposure from the crate root;
5. public unscoped lifecycle APIs before L9;
6. testkit/fuzz imports from lifecycle production source;
7. fuzz target sharing of a generic scaffold-only contract;
8. crash tests without localfs/testkit/wasm gating;
9. sleeps, thread spawns, or wall-clock waits in assurance tests.

### L8P-D: Verify Fuzz And Corpus Routing

Closeout should verify:

1. `lifecycle_recovery` target calls `check_lifecycle_recovery_fuzz_contract`;
2. `lifecycle_maintenance` target calls
   `check_lifecycle_maintenance_fuzz_contract`;
3. `lifecycle_retention` target calls `check_lifecycle_retention_fuzz_contract`;
4. no lifecycle target calls a scaffold-only contract;
5. each corpus directory has the exact required seed names;
6. seed bytes execute the intended success/failure/defer routes under normal
   tests;
7. fuzz binaries compile without requiring nightly fuzz execution.

### L8P-E: Record Sensitivity Probes

Add a final sensitivity ledger in `m4-l8-porting-log.md`.

Each row must include:

1. probe id;
2. mutation target file and function;
3. mutation description;
4. implemented test, generated counter, or structural guard that would catch the
   mutation;
5. verification command;
6. result;
7. live-mutation status.

Minimum probes:

| Probe | Mutation | Expected failure |
|---|---|---|
| S1 | Cache mode claims durable recovery. | Cache open/outcome/source guard tests fail. |
| S2 | Durable open creates objects before capability validation. | Capability ordering tests fail. |
| S3 | Bootstrap failure leaves runtime recovering instead of failed. | Recovery/bootstrap tests fail. |
| S4 | Recovered visible version advances beyond trusted checkpoint/WAL facts. | Bootstrap/generated recovery tests fail. |
| S5 | WAL tail repair runs in strict mode. | Recovery strict-tail tests fail. |
| S6 | Missing snapshot in lossy mode is reported healthy. | Recovery health/fault tests fail. |
| S7 | Flush advances flush watermark or truncates WAL directly. | Flush direct/source tests fail. |
| S8 | Checkpoint rejects opaque engine-owned sections. | Checkpoint recovery tests fail. |
| S9 | WAL truncation deletes records above proven watermark. | Generated checkpoint/crash tests fail. |
| S10 | Materialization uses naked layer index after enqueue. | Table rewrite/materialization tests fail. |
| S11 | Retention deletes reachable table objects. | Retention generated/direct tests fail. |
| S12 | Snapshot pruning deletes live manifest snapshot. | Snapshot pruning tests fail. |
| S13 | Purge skips fresh quarantine proof. | Purge/quarantine tests fail. |
| S14 | Repair mutates branch or invents missing objects. | Repair tests fail. |
| S15 | Close starts ordinary maintenance after close requested. | Close/generated tests fail. |
| S16 | Durable close releases writer guard before sync failure is resolved. | Durable close tests fail. |
| S17 | Fuzz targets all call a shared scaffold. | Fuzz inventory/source guard fails. |
| S18 | Crash tests lose localfs/wasm gating. | Source guard fails. |
| S19 | Lifecycle production imports testkit/fuzz. | Source guard fails. |
| S20 | Lifecycle source imports product/engine modules. | Source guard fails. |

If a probe is verified by existing tests rather than a live mutation run, mark
it `Covered-by-test`. Do not claim `Mutation-run` unless the mutation was
actually applied, failed, and reverted.

### L8P-F: Execute Command Matrix

Run and record the full L8 closeout matrix:

1. formatting;
2. lifecycle unit tests;
3. lifecycle generated properties;
4. lifecycle maintenance/recovery integration tests;
5. lifecycle fault tests;
6. lifecycle fuzz inventory tests;
7. lifecycle source guards;
8. localfs crash/reopen tests;
9. default/all-features/no-default checks;
10. wasm no-default compile check;
11. fuzz binary compile check;
12. cargo-hack feature powerset if available;
13. clippy all targets/all features;
14. whitespace check.

Command evidence belongs in the porting log, not in automated tests that assert
textual command logs.

### L8P-G: Final Deferral Map

Record non-L8 work as explicit deferrals:

1. public lifecycle/open/close API wrappers: L9;
2. product open/close/recovery wording: engine-next/L9;
3. primitive reconstruction callbacks: engine-next;
4. background worker threads and scheduler policy: engine-next/post-V1;
5. process-kill matrix across every phase in default CI: post-V1 optional
   assurance;
6. distributed object-store lease/fencing races: object backend work;
7. StrataHub sync/push/pull behavior: StrataHub integration layers;
8. query/index/search side effects: later query/index layers;
9. public maintenance commands: L9 or product API;
10. feature-depth/performance budget tuning: later architecture review.

## Exit Gate

L8P can close when:

1. closeout inventory tests pass;
2. source guards pass;
3. fuzz inventory and fuzz binary build pass;
4. generated, fault, crash, recovery, maintenance, and lifecycle unit suites
   pass;
5. clippy, fmt, and whitespace checks pass;
6. wasm/no-default and all-features checks pass or a tool-environment blocker is
   recorded;
7. sensitivity probes are recorded with concrete evidence;
8. final deferrals are explicit and not hiding L8 correctness bugs;
9. porting log marks L8A through L8P closeout status accurately.
