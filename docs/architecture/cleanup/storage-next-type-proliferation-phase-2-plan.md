# Storage-Next Type Proliferation Phase 2 Plan

Status: historical cleanup plan; temporary inventory guard retired

## Context

The first storage-next type cleanup phase established the workflow, split large
branch-state ownership areas, localized some private helpers, and added
closeout guards. It did not aggressively reduce the total type count.

The generated inventory tooling was temporary cleanup scaffolding for this work.
It is now retired and should not be regenerated.

Historical post-CLN-T10 inventory:

| Metric | Count |
|---|---:|
| All struct/enum definitions | 932 |
| Production struct/enum definitions | 658 |
| Cleanup-target production definitions | 517 |
| Public API definitions | 94 |
| Durable format definitions | 47 |

The first phase reduced cleanup-target production definitions from 547 to 517,
a net reduction of 30. That is useful, but it is not enough to solve the
proliferation problem.

Phase 2 should therefore be a direct operation-family collapse phase. It should
not spend most of its effort on file moves, facade reshaping, or testkit-only
cleanup.

## Goal

Reduce cleanup-target production definitions from 517 to 450 or lower while
preserving:

1. public API stability;
2. durable format stability;
3. storage error codes and source chains;
4. affected-object, retryability, health, and state-change facts;
5. proof-backed safety around deletion, pruning, recovery, publication, and
   visibility.

The target requires removing roughly 67 more production cleanup-target
structs/enums. This should come from private staging names, not from public API
or durable format contracts.

## Decision

Run Phase 2 operation by operation, not directory by directory.

The review question for each type is:

> Is this a real boundary type, or is it just a staging name inside one
> operation?

If it is a staging-only type, collapse it. If it protects a durable fact,
recovery fact, proof, or layer boundary, keep it and document why.

## Non-Goals

Phase 2 does not:

1. remove public `api/*` request, summary, outcome, diagnostics, or error
   types;
2. change durable `format/*` structs/enums or golden-vector behavior;
3. change storage semantics;
4. change error codes unless the existing mapping is demonstrably wrong;
5. add compatibility paths to old storage;
6. broaden facade re-exports for convenience;
7. hide type count by moving definitions into tests or new modules.

## Slice Rules

Each Phase 2 slice must:

1. stay within the normal 1,500 net LOC slice budget, or split before landing;
2. cover one operation family;
3. identify the operation boundary type that remains;
4. list removed staging types in the commit message or cleanup ledger;
5. keep public API and durable format types unless a separate approved plan
   replaces them;
6. preserve existing behavior tests where possible, with test edits limited to
   imports and naming fallout;
7. document any new named storage boundary type in the slice notes;
8. avoid broad facade re-growth unless the slice explains the boundary need.

Proof merges require extra care:

1. proof merges must land in proof-only slices;
2. the merged type must restate the combined invariant it protects;
3. an unchanged or added test must pin that invariant;
4. if no test exists, defer the merge and write a behavior-test plan first.

## Verification Floor

Every slice should run focused tests for the touched operation plus:

```sh
cargo test -p strata-storage-next --locked --test api_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
```

If the slice touches durable format code, also run:

```sh
cargo test -p strata-storage-next --locked --test format_golden
```

If the slice touches feature-gated testkit/fault code, also run the relevant
feature-gated test target explicitly, for example:

```sh
cargo test -p strata-storage-next --locked --features testkit --test api_properties
cargo test -p strata-storage-next --locked --features fault-injection --test api_faults
```

## Retired Guard Policy

The temporary inventory guard is no longer part of the storage-next gate. Do
not generate new inventory artifacts or update retired closeout ceilings.

For future cleanup slices, keep the same intent through lighter-weight gates:

1. run focused operation tests and the relevant source guards;
2. keep public API and durable format surfaces explicit;
3. document any new named boundary type in the implementation notes;
4. reject convenience facade re-exports unless they are a real layer boundary;
5. rely on review to challenge private operation scaffolding.

## Roadmap

| Code | Unit | Primary files | Primary action | Expected reduction |
|---|---|---|---|---:|
| `CLN2-T1` | Manifest service collapse | `service/manifest.rs` | Collapse private load/write/recovery staging shells that only carry facts through one call chain. Keep manifest service, manifest errors, and durable manifest facts. | 4-8 |
| `CLN2-T2` | Table service collapse | `service/table.rs` | Keep table service and object facts; collapse byte-source, inventory, and reader wrappers that do not cross service/lifecycle boundaries. | 3-6 |
| `CLN2-T3` | Lifecycle checkpoint collapse | `lifecycle/checkpoint.rs` | Keep checkpoint outcome and real proof types; collapse flush-watermark validation/context staging where it is not a boundary. | 4-7 |
| `CLN2-T4` | Lifecycle recovery collapse | `lifecycle/recovery.rs` | Collapse checkpoint/WAL/quarantine/table recovery stage structs that only feed one installation path. Preserve recovery health and source-error facts. | 4-8 |
| `CLN2-T5` | Lifecycle quarantine collapse | `lifecycle/quarantine.rs` | Keep unsafe-deletion proofs; collapse request/report shells that duplicate branch/object facts across quarantine, purge, and repair. | 5-9 |
| `CLN2-T6` | Quarantine reconcile collapse | `service/quarantine/reconcile.rs` | Merge classification/detail shells that are only display or routing names. Keep mismatch and corruption facts visible. | 4-8 |
| `CLN2-T7` | Branch facts/read collapse | `branch/facts.rs`, `branch/read.rs` | Collapse private observed-fact, sort-key, and candidate types that do not guard ownership or visibility invariants. | 5-10 |
| `CLN2-T8` | Commit internals collapse | `commit/batch.rs`, selected `commit/*` internals | Keep commit runtime boundaries and error facts; collapse validation/result shells that do not cross a layer boundary. | 4-8 |
| `CLN2-T9` | Table runtime collapse | `table/cache.rs`, `table/compaction.rs`, `table/facts.rs` | Collapse table-local report/stats/policy shells that do not cross table/lifecycle boundaries. | 5-9 |
| `CLN2-T10` | Facade and allowance tightening | `lifecycle/mod.rs`, `commit/mod.rs`, `table/mod.rs`, `service/mod.rs` | Remove dead re-exports and stale `allow(unused_imports)` / `expect(dead_code)` markers after prior reductions. | 0-3 |

Expected total reduction if slices land near the middle of their ranges:
approximately 42 to 76 cleanup-target production types.

## First Slice Recommendation

Start with `CLN2-T1`, the manifest service collapse.

Reason:

1. `service/manifest.rs` is the largest production file by LOC in the current
   inventory;
2. it has 14 type definitions;
3. it is not public API;
4. it is not itself durable format code;
5. it is likely to contain private load/write/recovery staging types that can
   be collapsed without touching branch or lifecycle proof semantics first.

`CLN2-T1` should begin by listing every struct/enum in `service/manifest.rs`
and classifying each as:

1. keep boundary;
2. keep proof/fact;
3. collapse staging;
4. localize private helper;
5. defer because behavior tests are insufficient.

Only after that classification should code edits start.

## Per-Slice Checklist

Before editing:

1. Which type is the actual boundary type for this operation?
2. Which types are only staging names inside one call chain?
3. Which types carry source errors, object names, branch IDs, commit versions,
   retryability, health, or publication windows?
4. Which proof types prevent unsafe deletion, pruning, publication, recovery,
   or visibility changes?
5. Which tests pin those invariants?
6. Which callers use the type outside its owning module?
7. Which parent re-exports can disappear after call sites use explicit modules?

After editing:

1. Did total type count go down?
2. Did cleanup-target production type count go down?
3. Did suffix-family counts go down?
4. Did parent re-export counts go down or stay flat?
5. Did scaffold allowance markers go down or stay flat?
6. Did focused behavior tests still pass?
7. Did focused behavior tests and source guards pass?

## Stop Conditions

Stop a slice and write a behavior plan instead if reducing a type would:

1. merge two externally observable outcome states;
2. drop an affected object name;
3. drop a source error or source chain;
4. change retryability;
5. change a durable publication, recovery, or quarantine fact;
6. weaken a stale-proof or fail-closed decision;
7. require changing a public API type.

The point of Phase 2 is to remove scaffolding, not to make semantic changes
under cleanup cover.
