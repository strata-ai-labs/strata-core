# L6K Implementation Plan: Snapshot Row Install

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L6/l6k-snapshot-row-install-test-plan.md`

## Objective

Add storage-level decoded snapshot row install mechanics to storage-next L6.

L6K receives already-decoded, row-native storage rows and installs them into
branch-local LSM state. It is the branch-state mutation primitive that L8 will
use after L3/L4 snapshot services have decoded and verified durable snapshot
bytes.

L6K is not a snapshot codec, durable snapshot reader, manifest publisher,
table-object publisher, recovery coordinator, or product import/export path.
Those remain L3/L4/L8/L9 responsibilities. L6K owns only generic row preflight,
L5 table construction, branch-state staging, and all-or-nothing in-memory
install.

L6K establishes:

1. a row-native install request that contains `StorageRow` values, not
   primitive DTOs;
2. explicit branch target policy for existing and missing branches;
3. full preflight over every branch group before any branch state is mutated;
4. deterministic L5 table build plans and output identities;
5. all-or-nothing branch-state replacement for the install batch;
6. preservation of latest, version-bounded, timestamp-bounded, history, prefix,
   and range read semantics after install;
7. install outcome facts for L8;
8. generated model coverage and source-guard updates.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L6/l6b-branch-row-identity-read-bounds-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L6/l6c-branch-local-mutable-frozen-state-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L6/l6g-timestamp-ttl-visibility-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L6/l6i-reachability-shared-table-refs-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
10. `crates/storage/src/durability/decoded_snapshot_install.rs`
11. `crates/storage-next/src/row/mod.rs`
12. `crates/storage-next/src/table/{builder.rs,config.rs,facts.rs,key.rs,reader.rs}`
13. `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
14. `crates/storage-next/src/format/snapshot.rs`
15. `crates/storage-next/src/service/snapshot/`

## Existing-Code Source Map

| Current file | L6K evidence | L6K action |
|---|---|---|
| `crates/storage/src/durability/decoded_snapshot_install.rs` | Current storage accepts decoded snapshot groups, validates empty groups, empty spaces, zero versions, and duplicate row identities before mutation, then installs rows into `SegmentedStore`. | Preserve the generic decoded-row install boundary and preflight-before-mutation rule. Replace old primitive `DecodedSnapshotEntry`/`TypeTag` vocabulary with storage-next `StorageRow` and physical-key branch facts. |
| `crates/storage/src/durability/decoded_snapshot_install.rs` tests | Current tests cover multi-branch install, tombstone preservation, empty plan, invalid groups, empty spaces, zero versions, duplicate rows, row-count mismatch, and error diagnostics. | Port the behavioral envelope as branch-local L6 tests over `StorageRow`, L5 table readers, and `BranchLocalState`, without old `Value`, `Key`, `Namespace`, or `TypeTag`. |
| `crates/storage-next/src/format/snapshot.rs` | L3 owns durable snapshot container mechanics and row-native snapshot bytes. | L6K must not parse bytes or section envelopes; it receives decoded `StorageRow` values only. |
| `crates/storage-next/src/service/snapshot/` | L4 owns snapshot object publication/listing/pruning and backend capability checks. | L6K must not import services, object layout, backend handles, or snapshot object names. L8 composes L4 snapshot services with L6K. |
| `crates/storage-next/src/table/builder.rs` | L5 builds immutable table artifacts and validates row limits, key ordering, CRC/facts, and table byte facts. | L6K uses L5 builders/readers to stage branch-owned immutable tables. |
| `crates/storage-next/src/branch/state.rs` | L6 branch state already installs L0/nonzero immutable tables, captures pinned views, tracks facts, and emits reachability. | Add snapshot install request/preflight/staging/outcome helpers that reuse existing branch install invariants. |
| `crates/storage-next/src/branch/facts.rs` | L6I reachability facts represent branch-owned and inherited table refs. | Snapshot install outcomes expose enough table identities/counts for L8 to publish durable reachability after L6 state is staged. |

## Scope

L6K implements:

1. row-native snapshot install request and outcome types;
2. branch target policy:
   - reject missing branch;
   - create missing branch from a supplied `BranchRuntimeConfig`;
   - reject non-empty target branch unless an explicit replace mode is added and
     tested;
3. per-branch row groups derived from `StorageRow.physical_key().branch_id()`;
4. validation that every supplied row targets the declared branch group;
5. strict internal-key ordering validation within each branch group;
6. duplicate internal-key rejection across the entire install batch;
7. validation of commit version, timestamp, expiry, tombstone, and row branch
   facts using L6B/L6G helpers;
8. deterministic output table identity generation from a storage-owned seed,
   branch id, table index, and row fingerprint;
9. table chunking/build through L5 using `TableBuilderConfig`;
10. output artifact decode into `ImmutableTableReader` before mutation;
11. staged replacement states for every target branch before any visible swap;
12. all-or-nothing mutation across every target branch in the install batch;
13. install outcome facts:
    - target branch ids;
    - branches created;
    - branches replaced;
    - rows installed;
    - tables created;
    - max commit version;
    - timestamp min/max;
    - table identities;
    - recovery classification;
14. reachability snapshot compatibility for installed tables;
15. generated branch-LSM counters and source-guard updates.

L6K does not implement:

1. snapshot byte decoding;
2. snapshot section routing;
3. primitive DTO conversion;
4. StrataHub import/export or push/pull behavior;
5. durable table object publication;
6. durable branch manifest publication;
7. WAL replay or WAL truncation;
8. crash-window reconciliation;
9. backend IO;
10. object layout;
11. branch deletion or durable branch replacement orchestration;
12. commit-version allocation;
13. product branch naming or product restore commands;
14. old `DecodedSnapshotEntry`, `TypeTag`, `Value`, `Key`, `Namespace`, or
    `VersionedValue`.

## Core Rule: Snapshot Install Is A Branch-State Staging Operation

L6K should treat decoded snapshot install as a complete staged branch-state
mutation.

The implementation should:

1. validate the whole install request;
2. build all required table artifacts;
3. decode all built artifacts into readers;
4. construct all replacement branch states in memory;
5. validate all replacement facts and reachability facts;
6. swap the staged replacement states into the target map only after every
   previous step succeeds.

If any step fails, all existing branch states must remain byte-for-byte
unchanged from the caller's perspective, and no new branch state may become
visible.

## Proposed Type Surface

Exact names may change, but L6K should add equivalents of:

```text
BranchSnapshotInstallRequest
  output_identity_seed
  missing_branch_policy
  target_state_policy
  table_builder_config
  rows

BranchSnapshotMissingBranchPolicy
  Reject
  Create { config }

BranchSnapshotTargetStatePolicy
  RequireEmpty
  ReplaceExistingEmptyOrSnapshotRestoredOnly  # optional future expansion

BranchSnapshotInstallBranchPlan
  branch_id
  row_count
  table_count
  output_level
  output_identities
  max_commit_version
  timestamp_min
  timestamp_max

BranchSnapshotInstallPlan
  branch_plans
  row_count
  table_count

BranchSnapshotInstallOutcome
  recovery
  branch_outcomes
  rows_installed
  tables_created
  branches_created
  branches_replaced

BranchSnapshotInstallRecovery
  EmptyPlanNoop
  Installed
```

The public surface remains `pub(crate)`.

## Request Semantics

### Input Rows

Rows are storage-next `StorageRow` values. L6K must not accept engine DTOs or
old storage decoded entries.

Rows must already contain:

1. physical key with branch id;
2. storage space id;
3. user key bytes;
4. commit version;
5. commit timestamp;
6. expiry timestamp;
7. put/tombstone payload facts.

Rows may contain empty user values, high-bit key bytes, embedded-zero key bytes,
tombstones, `Timestamp::EPOCH`, and `Timestamp::MAX`.

### Grouping

L6K should group rows by physical branch id. It should not accept a separate
product branch name or dataset id.

Within a branch group, rows must be strictly sorted by
`TableInternalKeyBytes`. Duplicate internal keys are invalid, even if the
duplicate appears in a different chunk or source group.

If the initial implementation chooses a caller-provided grouped shape instead
of flat rows, the same validation still applies:

1. group branch id must match every row physical key branch id;
2. groups must have unique branch ids;
3. groups must be ordered deterministically by branch id;
4. rows inside each group must be strictly sorted.

### Target State Policy

V1 should start conservative:

1. installing into a missing branch is rejected unless the request explicitly
   says to create missing branches with a supplied `BranchRuntimeConfig`;
2. installing into a non-empty branch is rejected unless a later, explicit
   replace mode is designed and tested;
3. branch creation or replacement must be staged and all-or-nothing across the
   whole batch.

This avoids accidentally treating snapshot install as a merge, branch restore,
or product import feature. Product restore semantics belong above L6.

## Table Build And Identity Rules

L6K should build branch-owned immutable L0 tables through L5.

Rules:

1. output tables are child/local branch-owned tables, not inherited or
   materialization replacement refs;
2. output level is L0 unless a later snapshot format carries trusted level
   placement facts;
3. output identities include the request seed, branch id, table index, and row
   fingerprint so two branches with identical rows do not alias;
4. output identities must not collide with any existing reachable table
   identity in the target branch set;
5. all output artifacts are decoded with `ImmutableTableReader` before any
   branch-state mutation;
6. L5 builder/decode errors are preserved as `TableRuntime` sources.

If a snapshot carries prebuilt table objects in a later milestone, that is a
different install mode. L6K V1 installs decoded rows.

## Install Transaction

Suggested flow:

1. validate request-level config and policies;
2. group rows by branch id;
3. validate target branches and missing-branch policy;
4. validate every row branch id, internal-key ordering, duplicate key, and row
   metadata;
5. build deterministic per-branch table plans;
6. build and decode every output table through L5;
7. clone target branch states or create new empty states;
8. apply all output tables to the staged branch states;
9. validate staged branch facts and reachability snapshots;
10. swap every staged branch state into the caller-owned branch map;
11. return outcome facts.

The swap step should be the only visible mutation.

## Read Semantics

After a successful install, new read views over installed branches must behave
as if the rows had been committed historically:

1. latest point reads choose newest live row;
2. version-bounded reads choose the newest row at or below the requested
   version;
3. timestamp-bounded reads apply commit timestamp, tombstone, and TTL semantics;
4. history returns retained rows in newest-first order;
5. prefix scans group by physical key and suppress tombstoned/expired latest
   values;
6. range scans preserve storage-space boundaries and inclusive/exclusive bounds;
7. branch facts report max commit version and timestamp range from installed
   rows.

Pinned read views captured before install must continue to see the pre-install
state.

## Error And Recovery Facts

Add typed error/outcome variants for:

1. invalid snapshot install request;
2. missing branch rejected;
3. target branch not empty;
4. duplicate target branch group;
5. row branch mismatch;
6. duplicate internal key;
7. unsorted row group;
8. invalid row metadata;
9. output identity collision;
10. L5 table build/decode failure;
11. staged branch validation failure.

Outcome/recovery facts should distinguish:

1. empty plan no-op;
2. installed rows and tables;
3. created missing branches;
4. replaced empty target branches;
5. validation failed before staging;
6. staging failed before visible mutation.

No error or debug string should include row value bytes.

## Target Module Shape

Expected production layout after L6K:

```text
crates/storage-next/src/branch/
  mod.rs
  config.rs
  error.rs
  facts.rs
  identity.rs
  read.rs
  state.rs
  snapshot_install.rs  # optional if state.rs grows too large
  tests.rs
```

If `snapshot_install.rs` is added, keep it `pub(crate)` and update source
guards.

## Implementation Steps

### L6K-A: Source Map And Vocabulary

1. Add this implementation plan and test plan.
2. Add a porting-log section for decoded snapshot install.
3. Add request, policy, plan, outcome, and recovery/error vocabulary.
4. Update source guards to allow only L6K-owned snapshot row install
   entrypoints.

### L6K-B: Request Preflight

1. Validate missing-branch and target-state policies.
2. Group flat `StorageRow` inputs by branch id, or validate caller-provided
   branch groups.
3. Reject duplicate branch groups.
4. Reject rows whose physical-key branch id does not match their branch group.
5. Reject unsorted or duplicate internal keys.
6. Reject non-empty targets under the V1 require-empty policy.
7. Preserve diagnostics without value payload bytes.

### L6K-C: Table Build Staging

1. Extract or reuse a branch-local helper for building L0 `BranchOwnedTable`
   values from sorted rows.
2. Generate deterministic, branch-specific output identities.
3. Split rows into multiple tables only through explicit table-builder/chunk
   policy.
4. Decode each built artifact through `ImmutableTableReader`.
5. Reject output identity collisions before mutation.

### L6K-D: All-Or-Nothing Install

1. Build staged `BranchLocalState` replacements for every target branch.
2. Validate staged branch facts and reachability snapshots.
3. Swap all staged states into the caller-owned branch map in one logical step.
4. Return install outcome facts.
5. Ensure empty install reports a typed no-op and does not mutate.

### L6K-E: Generated Coverage And Guards

1. Extend `crates/storage-next/src/testkit/branch_lsm.rs` with generated
   snapshot install scripts and counters.
2. Extend `crates/storage-next/tests/branch_lsm_properties.rs` to require every
   L6K generated counter to be nonzero.
3. Extend source guards to allow L6K-owned entrypoints while continuing to
   reject backend, lifecycle, service, old storage, product DTO, and snapshot
   codec imports in production `branch/`.
4. Record sensitivity probes and verification commands in the porting log.

## Deferred

1. Durable snapshot byte decoding and object reads: L3/L4/L8.
2. Durable table publication and manifest publication: L4/L8.
3. Crash-window reconciliation: L8.
4. Snapshot restore product semantics: L9/product.
5. Installing prebuilt table-object snapshots: later snapshot format work.
6. Incremental snapshot merge into non-empty branches: later explicit policy.
7. Branch deletion/clear orchestration around restore: L8/L9.

## Verification Commands

Run at least:

```bash
cargo test -p strata-storage-next --locked --lib branch_snapshot
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

If L6K changes L5 builder behavior, also run:

```bash
cargo test -p strata-storage-next --locked --lib table::tests::builder
cargo test -p strata-storage-next --locked --test table_runtime_properties
```

## Exit Criteria

L6K is complete when:

1. decoded row snapshot install uses storage-next `StorageRow` only;
2. missing-branch and non-empty-target behavior is explicit and tested;
3. full request preflight runs before any branch-state mutation;
4. row branch mismatch, duplicate key, unsorted rows, and invalid metadata are
   rejected without mutation;
5. all output tables are built and decoded before branch visibility;
6. install is all-or-nothing across every target branch;
7. post-install reads match an independent row-chain model;
8. pinned old read views remain valid;
9. reachability facts include installed tables;
10. generated tests exercise nonzero snapshot install counters;
11. source guards prove L6K does not import snapshot codecs, services, backend,
    lifecycle, old storage, product DTO, or durable publication behavior;
12. the porting log records preserved old decoded-install behavior, intentional
    V1 changes, deferred durable work, and sensitivity probes.
