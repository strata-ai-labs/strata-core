# M4-L9 Implementation Plan: Storage API Boundary

Status: draft implementation plan

## Objective

Build the storage-next API boundary consumed by engine-next.

M4-L9 turns the storage-next runtime into a small, synchronous, engine-facing
storage contract. It wraps the lower storage layers without exposing WAL,
manifest, snapshot, table, branch-LSM, commit-runtime, lifecycle, backend, or
object-layout internals. Engine-next consumes L9 for open/create, reads,
commits, branch mechanics, maintenance, recovery facts, diagnostics, and close.

L9 is not a product API. It must not expose primitive DTOs, JSON/event/vector/
graph semantics, StrataHub behavior, IPC behavior, distributed durability
claims, or user-facing recovery wording. It is a storage boundary for engine
adapters and tests.

L9 also closes the storage-next visibility boundary:

1. lower modules remain implementation details;
2. storage-next exposes one coherent synchronous surface upward;
3. engine-next tests can use a fake L9-compatible persistence implementation;
4. product crates above engine cannot reach below engine into storage internals.

## Inputs

1. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
2. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
3. `docs/architecture/storage/l7-commit-runtime.md`
4. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
5. `docs/architecture/storage/l5-table-runtime.md`
6. `docs/architecture/storage/future-object-durable-guardrails.md`
7. `docs/architecture/strata-v1-architecture.md`
8. `docs/architecture/engine/testing-and-conformance-plan.md`
9. `docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md`
10. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
11. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
12. `crates/storage-next/src/api/`
13. `crates/storage-next/src/lifecycle/`
14. `crates/storage-next/src/commit/`
15. `crates/storage-next/src/branch/`
16. `crates/storage-next/src/table/`
17. `crates/storage-next/src/observability/`

## Existing-Code Source Map

The old storage and engine code provide API-shape evidence, but the porting rule
is to extract storage boundary mechanics, not legacy product vocabulary.

| Current file | Relevant evidence | Porting rule |
|---|---|---|
| `crates/storage/src/traits.rs` | Synchronous storage trait with latest/version/history/prefix reads and batched writes. | Preserve the synchronous storage-owned affordances. Do not expose old `Value`, `VersionedValue`, or product-shaped `WriteMode` directly. |
| `crates/storage/src/runtime_config.rs` | Runtime configuration, durability mode, memory limits, flush/compaction knobs. | Port storage configuration knobs that L8 actually supports. Engine translates product config into L9 config. |
| `crates/storage/src/memory_stats.rs` | Storage memory and cache diagnostic shape. | Port raw storage stats through L9 diagnostics, not product telemetry. |
| `crates/storage/src/pressure.rs` | Write pressure and compaction pressure facts. | Port as storage pressure facts. Engine decides product stall/backoff policy. |
| `crates/storage/src/segmented/mod.rs` | Branch-aware reads, writes, flush, compaction, and recovery helpers. | Expose branch mechanics through L9, while keeping L6/L8 internals private. |
| `crates/storage/src/segmented/recovery.rs` | Degraded recovery categories and recovery result shape. | Expose raw recovery health and source-chain facts. Avoid product wording. |
| `crates/storage/src/segmented/quarantine_protocol.rs` | Quarantine, reclaim, purge, and repair operations. | Expose storage maintenance requests and outcomes, not direct object mutation APIs. |
| `crates/engine/src/database/open.rs` | Product open sequencing and storage attachment. | Engine should call L9 open/create. Product access mode, registry wiring, and UX remain above L9. |
| `crates/engine/src/database/transaction.rs` | Product transaction command flow over storage writes. | L9 exposes commit batches and outcomes only. Public transaction sessions remain out of V1. |
| `crates/engine/src/database/branch.rs` and related branch helpers | Product branch names, merge/restore behavior, and storage branch mechanics. | L9 exposes storage branch IDs/generations and mechanical fork/delete/clear. Product branch workflows stay in engine. |
| `crates/engine/src/database/recovery.rs` | Engine-facing recovery reporting and primitive reconstruction. | L9 exposes storage recovery facts only. Engine maps those facts into product diagnostics. |

Storage-next now provides the lower-layer mechanics that L9 wraps:

1. L6 branch read views, inherited layers, materialization, reachability, and
   branch lifecycle state;
2. L7 commit batches, commit outcomes, conflict validation, timeline rows,
   replay, quiesce, and durable uncertainty facts;
3. L8 cache and durable local lifecycle, open/recovery/maintenance/close,
   table-manifest reachability, retention, quarantine, repair, budgets, lazy
   reads, and diagnostics.

## L9 Boundaries

L9 owns:

1. the `api` module as the only engine-facing storage entry point;
2. public storage runtime handle type;
3. open/create options and outcomes;
4. storage-mode and durability-policy selection as supported by L8;
5. read selectors and read outcomes;
6. commit batch builder and commit outcomes;
7. branch lifecycle requests and outcomes;
8. timeline resolution requests and outcomes;
9. maintenance/checkpoint/retention/quarantine/repair requests and outcomes;
10. recovery, health, stats, and pressure reports;
11. close and shutdown requests and outcomes;
12. error codes and source-chain exposure at the storage boundary;
13. testkit fake/faulting L9-compatible persistence helpers;
14. source guards that prevent engine/product crates from importing private
    storage modules.

L9 must not own:

1. backend IO implementation;
2. WAL, manifest, snapshot, table, sidecar, or object-layout formats;
3. branch-LSM install or materialization algorithms;
4. commit allocator internals;
5. lifecycle recovery internals;
6. product DTOs or primitive semantics;
7. public transaction sessions;
8. durable transaction IDs;
9. distributed locks or consensus;
10. object-store provider behavior;
11. merge/cherry-pick/revert/restore product workflows;
12. user-facing recovery language;
13. automatic product policy for maintenance scheduling;
14. async runtimes or background thread ownership.

## API Shape Principles

1. **Synchronous only.**
   Public storage APIs must not expose `async`, `Future`, tokio, async-std, or
   runtime-specific types.

2. **Opaque storage atoms.**
   Branch IDs, commit versions, timestamps, storage-space IDs, keys, and byte
   values are storage atoms. Product concepts are encoded above L9 before they
   become storage rows.

3. **Result-first errors.**
   Ordinary invalid input, conflict, unsupported capability, corruption,
   durability uncertainty, recovery degradation, and closed-runtime access return
   typed storage errors. Panics are bugs.

4. **No lower-layer type leakage.**
   L9 may expose stable copies or summaries of lower-layer facts, but not
   service, format, table, commit-runtime, lifecycle, backend, or layout types
   directly.

5. **`pub` starts here.**
   L9 is the first storage-next layer allowed to expose crate-public API types.
   Lower modules stay `pub(crate)` unless an explicit implementation plan
   records an exception.

6. **Non-exhaustive by default.**
   Public enums and structs with future growth should be `#[non_exhaustive]`.

7. **Stable error codes.**
   Boundary errors expose a `code()` accessor with class/area/detail format.
   Tests assert codes and structured fields, not display strings.

8. **Engine-adapter neutral.**
   L9 should be usable by engine-next and by fake test implementations. It must
   not require engine-only callbacks or product registries.

## Delivery Parts

L9 is delivered in three logical parts with detailed slices. The slice labels
are planning labels only and should not appear in production Rust identifiers,
fixtures, or user-facing strings.

Detailed slice plans live under
`docs/architecture/implementation-plans/M4/L9/`.

### Part 1: Boundary Core

Boundary Core establishes the public storage shell.

It includes:

1. API module scaffold and public export policy;
2. storage error and result types;
3. storage open/create options;
4. storage runtime handle;
5. cache and durable local open/create wrappers over L8;
6. close and idempotent shutdown wrappers;
7. public outcome/fact vocabulary copied from lower-layer summaries;
8. source guards for visibility and forbidden vocabulary.

Exit gate:

1. engine-facing code can open cache and durable local storage through L9;
2. unsupported object-durable/distributed modes fail with typed errors;
3. no lower-layer implementation types leak through public signatures;
4. close is idempotent and surfaces L8 close facts;
5. all public boundary enums/structs are future-growth safe.

### Part 2: Data Operations

Data Operations exposes storage reads, commits, timeline resolution, and branch
mechanics.

It includes:

1. latest, version-bounded, timestamp-bounded, history, prefix, and range reads;
2. read-view pin or equivalent retention-safe selector;
3. storage commit batch builder;
4. put/delete/tombstone/TTL mutation types;
5. conflict/CAS selectors supported by L7;
6. commit durability options and outcomes;
7. timestamp-to-version and version-to-timestamp resolution;
8. retained timeline bounds;
9. storage branch create/fork/fork-at-retained-version/delete/clear/list
   mechanics;
10. branch-generation guards and pinned-view safety checks.

Exit gate:

1. reads match L6/L7 visibility and retained-history semantics;
2. commits route through L7/L8 and never mutate lower layers directly;
3. branch operations preserve generation and pinned-view safety;
4. timestamp and retained-history misses are distinguishable from not-found;
5. cross-branch atomic commit is rejected or absent.

### Part 3: Operations, Diagnostics, And Engine Testability

Operations and Diagnostics exposes storage maintenance and raw health facts.

It includes:

1. explicit checkpoint request/outcome;
2. flush, compaction, materialization, retention, quarantine, purge, repair, and
   WAL-growth maintenance requests;
3. automatic checkpoint/WAL-growth policy hooks added by L8Z;
4. storage diagnostics, pressure, memory/cache budget, lazy-read, table-manifest,
   branch lifecycle, and recovery facts;
5. raw storage health report and source chains;
6. API conformance helpers in testkit;
7. fake L9-compatible persistence for engine tests;
8. faulting wrapper over L9 for engine boundary tests;
9. closeout inventory and public API snapshot.

Exit gate:

1. maintenance requests use L8 outcomes and never expose L4/L5/L6 internals;
2. diagnostics are storage-shaped and product-neutral;
3. fake persistence implements the same trait/surface engine uses;
4. faulting wrapper can inject every boundary failure family engine needs;
5. engine-next tests can compile against L9 without private imports.

## Detailed Slice Order

### L9A: API Vocabulary And Visibility Boundary

Deliver:

1. `crates/storage-next/src/api/` module structure;
2. storage result/error code shape;
3. public storage atoms and request/outcome shells;
4. public export policy in `lib.rs`;
5. source guard for lower-layer visibility;
6. porting log section and API snapshot baseline.

Notes:

1. Do not expose lower-layer structs directly.
2. Prefer opaque wrapper structs when a lower-layer fact needs a stable boundary
   representation.
3. Keep constructors validating invariants at the boundary.

### L9B: Open, Runtime Handle, And Close

Deliver:

1. `StorageOpenOptions`;
2. cache/durable local open/create wrappers;
3. `StorageRuntime` handle;
4. `StorageOpenOutcome` and `StorageCloseOutcome`;
5. unsupported mode rejection for object-durable/distributed modes;
6. close idempotency and retryable close fact mapping.

Notes:

1. The runtime handle owns L8 cache or durable runtime internally.
2. L9 must not expose backend services, writer guards, or lifecycle shell types.
3. Cache mode must not claim durable recovery.

### L9C: Reads And Timeline Resolution

Deliver:

1. point read latest;
2. point read at version;
3. point read at timestamp;
4. retained history read;
5. prefix scan;
6. range scan;
7. retained timeline bounds;
8. timestamp-to-version and version-to-timestamp lookups;
9. read-view pin or equivalent selector if needed for retention safety.

Notes:

1. Timestamp miss due to insufficient history must not become not-found.
2. Tombstone and TTL visibility facts should be represented without product
   semantics.
3. Scans must have deterministic order and bounded result options.

### L9D: Commit API

Deliver:

1. commit batch builder;
2. mutation validation;
3. storage-space and branch validation;
4. commit durability options for cache/standard/always;
5. conflict/CAS selectors supported by L7;
6. commit outcomes including durable uncertainty and applied-not-visible facts;
7. no public transaction sessions;
8. no durable transaction IDs.

Notes:

1. L9 commits call L7/L8 commit runtime only.
2. Cross-branch atomic commits are rejected or not represented.
3. Public wording should not claim serializable isolation.

### L9E: Branch Lifecycle API

Deliver:

1. branch create/list/describe;
2. fork from current retained frontier;
3. fork at retained version or timestamp when L8Y support exists;
4. clear branch;
5. delete branch;
6. branch generation guard inputs;
7. pinned-view and retained-history rejection mapping;
8. branch cleanup outcome facts.

Notes:

1. Branch names, merge, restore, publish, and review workflows stay in engine.
2. L9 should expose storage branch mechanics and facts only.
3. Duplicate branch handling should be typed at the storage boundary.

### L9F: Maintenance API

Deliver:

1. checkpoint request/outcome;
2. flush request/outcome;
3. compaction and materialization requests/outcomes;
4. retention and snapshot-pruning request/outcome;
5. quarantine/reclaim/purge/repair requests/outcomes;
6. WAL-growth policy status and explicit trigger;
7. maintenance task status and deterministic drain for tests.

Notes:

1. Explicit maintenance APIs do not imply background policy beyond L8Z's minimal
   checkpoint/WAL-growth hook.
2. Table-object and table-manifest internals remain hidden.
3. Reclaim APIs require current proof facts and must fail closed.

### L9G: Diagnostics, Health, And Observability

Deliver:

1. storage health report;
2. recovery facts and degradation class;
3. memory/cache budget report;
4. table-manifest and table-object reachability summaries;
5. branch pressure and commit pressure facts;
6. maintenance queue stats;
7. lazy-read/cache counters;
8. source-chain access without product wording.

Notes:

1. Diagnostics are raw storage facts.
2. Product telemetry mapping happens above L9.
3. Avoid derived recommendations or user-facing explanations.

### L9H: Engine Testkit And Closeout

Deliver:

1. fake L9-compatible persistence implementation under `testkit`;
2. faulting L9 wrapper;
3. API conformance harness;
4. engine-next compile smoke using L9 only;
5. source guards that prevent engine-next from importing storage internals;
6. public API snapshot tests;
7. closeout command matrix and sensitivity ledger.

Notes:

1. Testkit must not become a second production API.
2. Fake persistence should be deterministic and storage-shaped.
3. Fault wrapper should inject boundary errors, not lower-layer internals.

## Runtime Handle Model

The L9 runtime handle should be a small owner of one opened storage runtime:

1. cache runtime;
2. durable local runtime with standard policy;
3. durable local runtime with always policy.

The handle provides synchronous methods. It should not clone mutable runtime
authority freely. If cloneable handles are needed later, the plan must define
ownership, locking, close behavior, and failure propagation explicitly.

Every public method should check runtime state before reaching lower layers and
map lower-layer failures into L9 errors with source-chain context.

## Error And Outcome Model

L9 errors should include:

1. invalid argument;
2. unsupported capability;
3. invalid lifecycle state;
4. branch not found;
5. branch already exists;
6. branch generation mismatch;
7. conflict;
8. retained history unavailable;
9. timestamp history unavailable;
10. durable uncertainty;
11. applied not visible;
12. recovery degraded;
13. recovery failed;
14. maintenance failed;
15. quarantine or retention proof rejected;
16. lower storage layer failure.

Every error exposes:

1. stable code;
2. storage area;
3. structured fields needed for engine mapping;
4. optional source-chain facts.

Display text is not a test oracle and should remain concise.

## Source Guard Policy

L9 source guards must prove:

1. `engine-next` imports storage-next only through the L9 public surface;
2. product crates above engine do not import storage-next;
3. storage-next lower modules are not `pub` except through explicit L9 exports;
4. L9 does not import engine, intelligence, inference, executor, SDK, CLI, or
   StrataHub crates;
5. L9 does not use async runtime types;
6. L9 production code does not contain product vocabulary for JSON, event,
   graph, vector, embedding, inference, prompt, model, or chat semantics;
7. lower modules do not import `api`;
8. testkit-only fake/faulting helpers are hidden behind test/testkit cfgs.

## Deferred Work

The following remain outside L9 V1:

1. production object-store/OpenDAL/S3 durability;
2. distributed locks or consensus;
3. multi-process/global commit version allocation;
4. public transaction sessions;
5. durable transaction IDs;
6. serializable isolation claims;
7. cross-branch atomic commits;
8. product branch workflows such as merge, cherry-pick, revert, restore, review,
   or publish;
9. product DTO encoding/decoding;
10. product recovery UX;
11. new physical table/WAL/snapshot formats;
12. L10 format-freeze and compatibility guarantees.

If any of these appear during L9 implementation, reject them or record a
separate implementation plan before proceeding.

## Porting Log

Create:

`docs/architecture/implementation-plans/M4/L9/m4-l9-porting-log.md`

Each slice entry must include:

1. shipped files;
2. boundary decisions;
3. old-code evidence used;
4. tests added;
5. sensitivity probes;
6. verification commands and pass/fail result;
7. deferred items with reason.

## Verification Matrix

Minimum closeout commands:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --locked --test api_conformance
cargo test -p strata-storage-next --features testkit --locked --test api_conformance
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --test api_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

Add wasm/no-default checks if the L9 public surface changes feature behavior.
Add an engine-next boundary smoke command once the engine-next crate exists in
the workspace.
