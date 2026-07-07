# Storage-Next Type Proliferation Reduction Plan

Status: historical cleanup plan; temporary inventory guard retired

## Retirement Note

The generated type-inventory tooling was temporary cleanup scaffolding. It
helped drive the storage-next type-reduction pass, but it is now retired and
should not be regenerated for future work.

Future storage cleanup should rely on focused source guards, behavior tests,
format goldens, review of new boundary types, and the design guidance in this
document rather than on generated inventory artifacts.

## Decision

Do this **operation by operation, grouped by layer/directory**, with a short
inventory pass before each operation-family cleanup.

Do not do this primarily feature by feature. Features are too wide and mix real
boundary types with private scaffolding. Do not do this primarily directory by
directory either. Directory cleanup is useful for sequencing, but by itself it
only redistributes the same type count into smaller files.

The review unit is:

1. one operation family;
2. one owning layer;
3. one behavior-preserving commit when possible;
4. a before/after type and re-export count.

Use the dedicated cleanup prefix `CLN-T*` for this work. PRs should keep the
normal engineering limit of 1,500 net LOC per slice. If an extraction would
exceed that budget, split the extraction by operation and land multiple
`CLN-T*` slices rather than opening a single large file-move PR.

Interlock with
`docs/architecture/cleanup/storage-next-file-comment-rollout-plan.md`: for a
directory that is about to be split or type-reduced, do the type-reduction split
first, then add or update file comments on the post-split files. Comments may
be the final step of a split slice; do not write detailed comments for files
that are scheduled to be split immediately.

## Goal

Reduce unnecessary struct/enum proliferation in `crates/storage-next` without
weakening storage correctness, recovery evidence, durable format boundaries, or
public API stability.

The problem is not that storage-next uses typed values. Typed values are
appropriate when they cross a layer boundary, validate caller input, preserve a
source error, or record durable/recovery facts. The problem is that too many
private operation steps have grown boundary-shaped type families:

1. `Request`;
2. `Plan`;
3. `Outcome`;
4. `Recovery`;
5. `Candidate`;
6. `PreparedOutput`;
7. operation-local `Kind` / `NoopReason` / `Invalidity`;
8. proof or attestation types for every invariant.

This violates the guidance in
`docs/architecture/storage-next/implementation-patterns.md`: private small
operations should stay small instead of creating a unique type family for each
step.

## Strategy Decision

Use **operation-family cleanup inside a layer**, not pure feature-by-feature or
directory-by-directory cleanup.

### Why Not Feature By Feature

Feature-by-feature cleanup is too broad for this problem. A feature such as
branching, compaction, maintenance, or diagnostics crosses multiple modules and
contains both legitimate boundary types and private scaffolding. Cleaning by
feature risks changing behavior and type boundaries at the same time.

### Why Not Directory By Directory

Directory-by-directory cleanup tends to move types around without reducing
them. For example, splitting `branch/state.rs` is necessary, but a file split by
itself could leave the same `Request`/`Plan`/`Outcome` proliferation in smaller
files.

### Preferred Unit

Clean one **operation family** at a time:

1. identify its public or layer-boundary contract;
2. keep only the types that enforce that contract;
3. inline or merge private one-call-site scaffolding;
4. reduce module re-exports to the remaining boundary surface;
5. run the existing behavior tests before moving to the next operation.

Directories still matter for sequencing. Start in the highest-proliferation
directories, but each cleanup pass should be scoped to one operation family.

## Operating Model

Each cleanup pass follows the same loop:

1. **Inventory**: list the operation's structs, enums, constructors, re-exports,
   call sites, and tests.
2. **Classify**: mark each type as keep, localize, merge, or inline.
3. **Fence behavior**: identify the tests that prove the operation's safety
   contract before editing.
4. **Reduce surface**: remove parent re-exports first, then collapse private
   scaffolding.
5. **Split files only where useful**: split large files to restore ownership,
   not to hide unchanged proliferation.
6. **Verify**: run the targeted unit tests, source guards, format, and clippy.
7. **Record**: update the cleanup ledger with before/after counts and any
   intentionally kept types.

If a pass needs semantic changes, stop and write a normal implementation plan
for that behavior change. This cleanup plan is for type and ownership reduction,
not for changing storage semantics.

Every PR should run the narrow tests for the touched operation plus this common
verification floor unless the PR description records a narrower rationale:

```sh
cargo test -p strata-storage-next
cargo test -p strata-storage-next --test format_golden
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Non-Goals

This cleanup must not include:

1. physical format changes;
2. public API removals or behavior changes;
3. error-code changes except to fix a documented contradiction;
4. test additions or removals beyond import-path edits and cleanup inventory
   guards;
5. executor, engine, or product behavior changes;
6. broad rewrites that combine type cleanup with semantic work.

## Type Classification

Every type touched by this cleanup should be classified before editing.

### Keep

Keep types when at least one condition is true:

1. Public API or public testkit surface.
2. Durable format, manifest, WAL, snapshot, table-object, or persisted proof.
3. Layer-boundary request or outcome used by multiple modules.
4. Error enum that preserves source errors or recovery-critical facts.
5. Validated configuration or option object reused across callers.
6. Proof token that is passed across a mutation boundary and prevents unsafe
   deletion, pruning, or visibility changes.

### Localize

Move types out of broad module facades when they are legitimate but only local
to one operation:

1. sort keys;
2. staging structs;
3. test fixtures;
4. temporary row collectors;
5. private grouping structs used only inside one implementation file.

These can remain structs, but they should not be re-exported from a parent
module.

### Merge

Merge type families when several private types are just phases of the same
operation and are not independently validated or reused. Common candidates:

1. `Candidate` + `Plan` when candidate selection is immediately consumed;
2. `PreparedOutput` + `Outcome` when prepared output is not reusable;
3. separate proof enums that are always bundled together;
4. operation-local `Recovery` enums that only choose a boolean retry path.

### Inline

Inline types when they are:

1. private;
2. used by one function or one small call chain;
3. not validating invariants;
4. not preserving recovery facts;
5. not named in tests as part of a behavior contract.

## Roadmap

This is the single roadmap for the cleanup. Each row is a PR-sized or
sub-PR-sized unit; if the measured diff would exceed 1,500 net LOC, split the
row further before opening the PR.

Historical note: the initial inventory and closeout slices used generated
type-inventory artifacts during the cleanup pass. That machinery has been
retired. The remaining guidance is qualitative: keep new types at real
boundaries, avoid private operation scaffolding, and prove cleanup with focused
source guards and behavior tests.

| Code | Unit | Primary files | Primary action | Expected result |
|---|---|---|---|---|
| `CLN-T0` | Inventory baseline | `crates/storage-next/src/**` | Historical: added repeatable inventory tooling during cleanup | Retired after cleanup completion |
| `CLN-T1` | Branch facade | `branch/mod.rs` | Remove broad re-exports, delete existing speculative `allow(unused_imports)` blocks as exports disappear, and update call sites to explicit submodule imports | Smaller branch public-in-crate surface |
| `CLN-T2A` | Branch state extraction: append/rotation | `branch/state.rs` | Move append and active/frozen rotation code into owned modules without semantic changes | First file-size reduction under budget |
| `CLN-T2B` | Branch state extraction: fork/read hooks | `branch/state.rs` | Move fork and read-facing helper code into owned modules without semantic changes | Branch state ownership gets clearer |
| `CLN-T2C` | Branch state extraction: materialization | `branch/state.rs` | Move materialization code into an owned module without collapsing types yet | Handle-based safety remains isolated |
| `CLN-T2D` | Branch state extraction: compaction | `branch/state.rs` | Move compaction code into an owned module without collapsing types yet | Compaction can be reduced separately |
| `CLN-T2E` | Branch state extraction: snapshot/recovery | `branch/state.rs` | Move snapshot install and table-manifest recovery code into owned modules without semantic changes | No single branch-state file remains above threshold |
| `CLN-T3` | Branch compaction family | `branch/state/*`, `branch/pruning.rs` | Collapse private candidate/prepared/recovery scaffolding after caller analysis | Fewer compaction-only types |
| `CLN-T4` | Branch materialization family | `branch/state/*` | Keep stable handles, merge private request/intent wrappers only where they do not protect layer-index drift | No materialization safety regression |
| `CLN-T5` | Snapshot install/recovery family | `branch/state/*` | Merge local group/outcome/recovery wrappers where callers do not need all layers | Smaller snapshot recovery family |
| `CLN-T6A` | Branch tombstone/TTL proof review | `branch/pruning.rs`, `branch/facts.rs` | Review and, if safe, merge only tombstone/TTL proof types in a proof-only PR | Combined invariant restated and pinned by tests |
| `CLN-T6B` | Branch inheritance/shared-table proof review | `branch/pruning.rs`, `branch/facts.rs` | Review and, if safe, merge inheritance/shared-table proof types in a proof-only PR | Unsafe deletion remains proof-gated |
| `CLN-T6C` | Branch read/facts localization | `branch/read.rs`, `branch/facts.rs` | Localize sort keys, observed facts, and helpers that do not cross a boundary | Fewer parent re-exports |
| `CLN-T7A` | Maintenance executor | `lifecycle/maintenance.rs` | Keep task boundary, merge runner-only status wrappers | Less executor-family duplication |
| `CLN-T7B` | Checkpoint outcomes | `lifecycle/checkpoint.rs` | Reduce private checkpoint staging types while preserving success/debt distinctions | Checkpoint and WAL debt remain distinct |
| `CLN-T7C` | Flush outcomes | `lifecycle/flush.rs` | Reduce private flush staging types while preserving orphan versus uncertainty facts | Flush fact surface remains stable |
| `CLN-T8A` | Retention | `lifecycle/retention.rs` | Reduce private retention report/proof scaffolding without weakening fail-closed behavior | Table-object retention remains conservative |
| `CLN-T8B` | Quarantine/purge/repair | `lifecycle/quarantine.rs` | Merge duplicate context/report structs only after inventory-generation proof review | Unsafe purge/reclaim remains blocked |
| `CLN-T8C` | Rewrite publication | `lifecycle/rewrite.rs`, `lifecycle/compaction.rs` | Localize rewrite publication staging types | Rewrite object facts remain visible |
| `CLN-T8D` | Budget facts | `lifecycle/budget.rs` | Inline constant policy wrappers and localize private counters | Budget admission tests still prove bounded allocation |
| `CLN-T9A` | Manifest service | `service/manifest.rs` | Localize private load/write stage structs | Durable manifest format unchanged |
| `CLN-T9B` | Table service | `service/table.rs` | Merge wrappers that duplicate backend publish facts | Table publication facts unchanged |
| `CLN-T9C` | WAL service | `service/wal.rs` | Localize mutation/replay stage structs | WAL fault-window tests unchanged |
| `CLN-T9D` | Quarantine service | `service/quarantine/*` | Reduce reconcile helper proliferation | Inventory mismatch behavior unchanged |
| `CLN-T9E` | Snapshot service | `service/snapshot.rs` | Localize snapshot publication helper types | Snapshot golden/fault tests unchanged |
| `CLN-T10` | API/testkit reports | `api/*`, `testkit/*` | Keep public API types; reduce duplicate testkit counters and dead shells | Clearer diagnostics and conformance surface |
| `CLN-T11` | Closeout guards | source guards and temporary inventory tooling | Historical: pinned cleanup progress with generated inventory artifacts | Retired; source guards and review now carry the regression pressure |

## Execution Rules

1. One operation family per commit.
2. No physical format changes in this cleanup.
3. No public API removal unless L9 documents the replacement.
4. No error-code changes unless the existing code is demonstrably wrong.
5. No behavior changes hidden inside file splits.
6. Every removed type must be either inlined, merged, or localized.
7. Every kept proof type must state the invariant it protects.
8. Every merged proof type must restate the combined invariant it now protects
   and link to the unchanged existing test that pins that invariant. If no such
   test exists, defer the merge to a separate behavior-test plan. Proof merges
   must land in proof-only PRs, not bundled with unrelated type reductions.
9. Every kept request/plan/outcome type must identify the layer boundary it
   crosses.
10. Outcome shape changes must preserve the same error code, source-error
    chain, affected object names, health facts, and state-change facts that
    callers observed before. Tests for these changes should keep bodies
    unchanged where possible; import-path edits are fine.
11. Existing speculative `allow(unused_imports)` and `expect(dead_code)` blocks
    must be removed as their dead exports or types disappear.

## Per-Operation Checklist

Before changing an operation family, answer these questions in the commit
message or cleanup ledger:

1. Which type is the actual boundary type for the operation?
2. Which types are only staging names inside one call chain?
3. Which tests prove the operation's safety invariant?
4. Which proof types prevent unsafe deletion, pruning, publication, or
   visibility changes?
5. Which re-exports are still necessary after call sites use explicit modules?
6. Which callers use the type outside its owning module? If the only external
   caller is one lifecycle or API file, document why that is a real boundary;
   if no external caller exists, treat it as private scaffolding unless it
   guards a proof or durable fact.
7. Did the operation-family surface and parent re-export count go down or stay
   intentionally flat?
8. Did the change preserve the operation boundary and avoid broad facade
   re-growth?

## Keep/Remove Examples

Examples that usually stay:

1. `Storage*Request`, `Storage*Summary`, and `StorageApiError` public API
   types.
2. Durable manifest, WAL, snapshot, table-object, and golden-vector format
   types.
3. Error enums carrying source errors, object names, branch IDs, commit
   versions, or publish windows.
4. Handle/proof tokens that protect against stale layer indices, stale
   inventory, stale retention proof, or unsafe reclaim.

Examples that should be challenged:

1. A `Candidate` immediately converted into a `Plan` in the same function.
2. A `PreparedOutput` that is never retried or inspected independently.
3. A `Recovery` enum that only maps to retry true/false in one match.
4. A `NoopReason` enum consumed by one test and one debug string.
5. An `Attestation` type that is always embedded in another proof and never
   travels alone.
6. A parent-module re-export used only to make tests shorter.
