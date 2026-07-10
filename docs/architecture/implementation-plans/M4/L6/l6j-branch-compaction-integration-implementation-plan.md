# L6J Implementation Plan: Branch Compaction Integration

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L6/l6j-branch-compaction-integration-test-plan.md`

## Objective

Add storage-level branch compaction integration to storage-next L6.

L6J connects the branch runtime to the generic L5 table compactor. It selects
branch-owned immutable table candidates, supplies branch-aware retention policy
facts to L5, installs compaction outputs into branch-owned levels, and emits
release facts for replaced table refs.

L6J is not a scheduler and not a durable cleanup slice. L8 decides when to run
compaction, publishes branch manifests, reconciles crash windows, and performs
physical object cleanup. L5 performs generic table merge/build mechanics. L6J
owns only branch-local candidate validity, branch read-safety policy, and the
atomic in-memory branch-state transition.

L6J establishes:

1. deterministic branch-owned compaction candidate facts;
2. explicit no-op decisions for branches or levels that are not compactable;
3. conservative branch-aware L5 compaction policies;
4. keep-all compaction as the always-safe baseline;
5. optional retention-pruning policy only behind explicit safety proofs;
6. output table decoding and descriptor validation before state mutation;
7. atomic replacement of selected old branch-owned table refs with output refs;
8. pinned read-view isolation across the install transition;
9. old-table release facts only after replacement reachability is visible;
10. generated model coverage and source-guard updates.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/l6g-timestamp-ttl-visibility-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L6/l6h-materialization-mechanics-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L6/l6i-reachability-shared-table-refs-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
10. `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
11. `crates/storage-next/src/table/{builder.rs,compaction.rs,facts.rs,key.rs,reader.rs}`
12. `crates/storage-next/src/row/mod.rs`
13. `crates/storage/src/segmented/compaction.rs`
14. `crates/storage/src/segmented/tests/compaction*.rs`
15. `crates/storage/src/segmented/tests/concurrency.rs`
16. `crates/storage/src/segmented/tests/materialize.rs`
17. `crates/storage/src/segmented/ref_registry.rs`

## Existing-Code Source Map

| Current file | L6J evidence | L6J action |
|---|---|---|
| `crates/storage/src/segmented/compaction.rs` | Current storage computes level scores, picks L0/tier/level candidates, merges rows, prunes old versions/tombstones/expired rows behind floors, swaps segment versions, writes manifests before deleting old files, and preserves concurrently flushed segments. | Port branch-local candidate and install invariants. Do not port filesystem paths, segment ids, manifest writes, rate limiting, environment logging, or physical deletion. |
| `crates/storage/src/segmented/tests/compaction*.rs` | Tests cover read parity, L0/L1 overlap, bottommost tombstone behavior, pruning floors, and output-level shape. | Rewrite as L6 direct/generated tests over storage-next rows, branch state, and L5 table compaction. |
| `crates/storage/src/segmented/tests/concurrency.rs` | Old compaction snapshot/swap tests preserve concurrent flushes and deleted-branch safety. | Represent this as L6 atomic preflight/install and pinned view isolation. Async locking and branch deletion orchestration stay above L6. |
| `crates/storage/src/segmented/ref_registry.rs` | Old compaction deletes old segments only after manifest/reachability safety. | Use L6I release facts for replaced table refs. L6J must not physically delete table objects. |
| `crates/storage-next/src/table/compaction.rs` | L5 provides `TableCompactor`, `TableCompactionSource`, `TableCompactionPolicy`, output artifacts, reports, and generic keep/drop decisions. | Feed L5 with branch-owned table rows and a branch-aware policy. Validate output artifacts before installing them. |
| `crates/storage-next/src/branch/state.rs` | Branch state owns active rows, frozen rows, branch-owned immutable levels, inherited layers, materialized replacement tables, and pinned read views. | Add candidate planning and output install helpers that mutate branch-owned levels atomically after all validation succeeds. |
| `crates/storage-next/src/branch/facts.rs` | L6I provides reachability snapshots, replacement refs, and release plans. | Emit removed-ref facts and replacement reachability facts for L8 after compaction install. |

## Scope

L6J implements:

1. branch-owned compaction request types for explicit caller-selected work;
2. deterministic candidate selection for branch-owned immutable tables;
3. no-op outcomes for missing branches, empty levels, single-table candidates
   that cannot be moved safely, and last-level compaction requests;
4. input validation that rejects inherited, materializing-source, unavailable,
   active, and frozen mutable sources as direct compaction inputs;
5. L0 compaction candidate shape: compact selected L0 tables together and keep
   newer unselected L0 tables in front;
6. L0-to-L1 candidate shape: include all selected L0 tables plus overlapping
   L1 tables needed to preserve L1 non-overlap;
7. L1+ candidate shape: select one or more non-overlapping tables at level N
   and overlapping tables at level N+1;
8. target-output-level facts and bottommost facts for retention policy;
9. branch compaction source ids that carry branch id, level, and table index
   without object paths;
10. a keep-all branch compaction policy that never drops storage rows;
11. a retention-proof policy surface for old-version, tombstone, and TTL drops;
12. rejection of unsafe pruning requests before L5 policy callbacks can drop
    rows;
13. L5 `TableCompactor` invocation using only L5 table APIs;
14. output artifact decode and `BranchOwnedTable` descriptor construction before
    mutating branch state;
15. output install into the target branch-owned level while removing exactly
    the selected old input/overlap tables;
16. preservation of unselected L0 tables and non-overlapping lower-level tables;
17. atomic reader-perspective state transition: either old state remains visible
    or all output tables are visible;
18. pinned read-view isolation across compaction install;
19. compaction outcome/report facts for input table refs, output table refs,
    rows kept, rows dropped, output bytes, split count, release candidates, and
    protected old refs;
20. generated branch-LSM counters and source-guard updates.

L6J does not implement:

1. background scheduling or score-based daemon loops;
2. durable manifest publication;
3. WAL-before-visible orchestration;
4. backend IO, object layout, filesystem paths, table object deletion,
   quarantine, repair, or retention execution;
5. cache invalidation outside L5 table runtime facts;
6. rate limiting;
7. environment-variable logging;
8. public API exposure;
9. old `VersionedValue`, product `Value`, old `Key`, `Namespace`, `TypeTag`, or
   product branch workflow behavior;
10. commit-version allocation;
11. snapshot row install;
12. StrataHub export/push behavior.

## Core Rule: Compaction Is A State Replacement, Not Cleanup

Compaction may rewrite branch-owned immutable tables into fewer or different L5
tables. That alone does not prove any storage row is safe to discard.

The baseline L6J policy is keep-all. Keep-all compaction must preserve every
retained storage row and therefore every supported read result:

1. latest;
2. version-bounded point reads;
3. timestamp-bounded point reads;
4. history with and without tombstones;
5. prefix scans;
6. range scans;
7. inherited shadowing interactions;
8. pinned read views captured before compaction.

Any row-dropping policy must be explicit and proof-backed. L6J must reject a
policy that would drop old versions, tombstones, or TTL-expired rows unless the
caller supplies storage-owned retention facts proving that no supported L6 read
can observe the dropped row or any row it suppresses.

## Candidate Model

The exact Rust names may change, but L6J should add equivalents of:

```text
BranchCompactionRequest
  branch_id
  kind
  retention_policy
  output_identity_seed

BranchCompactionKind
  CompactL0 { max_tables optional }
  CompactL0ToL1
  CompactLevel { level, table_index optional }
  CompactExplicit { input_refs, output_level }

BranchCompactionCandidate
  branch_id
  input_refs
  overlap_refs
  preserved_refs
  output_level
  bottommost_for_branch
  key_range
  source_count
  input_row_count

BranchCompactionOutcome
  branch_id
  candidate
  installed_output_refs
  removed_refs
  release_plan
  table_compaction_report
  recovery
```

Candidate facts must refer to branch-local table descriptors and table
identities. They must not contain object paths, backend handles, segment ids, or
durable manifest bytes.

## Candidate Selection Rules

### General Rules

1. Candidate selection reads only the current `BranchLocalState`.
2. Active and frozen mutable rows are not direct compaction inputs.
3. Inherited layers are not direct compaction inputs.
4. Materialized replacement tables are ordinary child-owned branch tables for
   future compaction.
5. A candidate with fewer than two total input tables is a no-op unless the
   candidate is an explicit metadata-only move that preserves all read and
   overlap invariants.
6. The last configured level cannot compact downward.
7. Selection is deterministic for the same branch state and request.
8. Selection must validate table descriptor identity, level, key range, and
   branch ownership before returning a candidate.

### L0 Compaction

L0 tables may overlap. L6J may compact all L0 tables, or a caller-selected
prefix/tier of L0 tables, into L0 or L1 depending on the request.

When output stays in L0:

1. selected old L0 tables are removed;
2. unselected L0 tables remain in their original relative order;
3. output tables are inserted at the oldest selected position so newer L0
   tables continue to shadow older rows;
4. output rows must preserve exact read parity.

When output moves to L1:

1. all selected L0 tables participate;
2. every overlapping L1 table participates;
3. non-overlapping L1 tables are preserved;
4. installed L1 tables must be sorted and non-overlapping by physical key range.

### L1+ Compaction

For a nonzero level N:

1. select one or more non-overlapping input tables from level N;
2. include overlapping tables from level N+1;
3. preserve non-overlapping tables in level N+1;
4. install outputs into level N+1 sorted by key range;
5. remove exactly the selected level N tables and overlapping level N+1 tables;
6. preserve all other branch-owned levels.

Round-robin compact pointers from the old implementation are scheduler hints,
not required L6 state for V1. If L6J adds them, they must remain storage-owned
facts and must not depend on wall-clock time or background threads.

## Retention Policy Surface

L6J should define a storage-owned policy surface equivalent to:

```text
BranchCompactionRetentionPolicy
  KeepAll
  ProofBacked {
    version_floor optional
    timestamp_floor optional
    allow_tombstone_elision
    allow_ttl_elision
    bottommost_required
  }
```

Keep-all:

1. always safe;
2. never drops old versions;
3. never drops tombstones;
4. never drops expired rows;
5. is the first implementation target.

Proof-backed pruning:

1. must be rejected when proof facts are absent or internally inconsistent;
2. may drop an older version only when retained-version coverage proves reads
   below the floor are unavailable and a newer retained row for the same
   physical key remains where required;
3. may drop a tombstone only when no lower branch-owned or inherited row can be
   resurrected and retained-version/timestamp facts prove the tombstone cannot
   be requested as history;
4. may drop a TTL-expired row only when timestamp/as-of retention facts prove no
   supported timestamp read can observe it;
5. must not use wall-clock time;
6. must record every dropped row reason from L5 in the branch outcome.

If proof-backed pruning is too large for the first L6J patch, the shipped API
should expose `KeepAll` and typed rejection for every pruning request. The test
plan still treats unsafe pruning rejection as mandatory.

## Install Transaction

L6J should split compaction into preflight/build and install phases:

1. capture candidate table refs from the current branch state;
2. build L5 compaction sources from the selected branch-owned table readers;
3. run L5 compaction with the selected branch retention policy;
4. decode every output artifact into an `ImmutableTableReader`;
5. build every `BranchOwnedTable` descriptor for the target output level;
6. validate output identities against existing owned and inherited reachable
   table identities, then validate output level invariants before mutating
   state;
7. revalidate that selected input refs still match the current branch state;
8. install outputs and remove selected old refs in one state mutation;
9. refresh branch facts;
10. produce removed-ref release facts using L6I reachability vocabulary.

If any step before the state mutation fails, branch state must remain unchanged.

If revalidation fails because the branch changed since candidate capture, return
a typed stale-candidate error and leave state unchanged. L8 can reschedule.

## Read-View Semantics

`BranchReadView` is pinned. A view captured before compaction must keep its old
table readers and inherited layers, even after `BranchLocalState` installs
compaction outputs.

New views captured after compaction must see the replacement tables and must not
see removed old table refs. Keep-all compaction must make old and new views
return identical read results for every supported L6 read mode.

Proof-backed pruning may intentionally change storage-history availability only
inside the bounds of explicit retention facts. It must never change latest or
as-of results that the proof says are still retained.

## Reachability And Release Facts

After a successful compaction install:

1. output tables are reachable from the owning branch;
2. removed old table refs become release-plan inputs;
3. release candidates are emitted only after replacement reachability is visible
   in the branch snapshot;
4. shared/inherited old tables remain protected by L6I aggregate facts;
5. runtime registry disagreement blocks release;
6. L6J does not delete objects or decrement durable refs directly.

Compaction output refs should be classified as ordinary branch-owned refs, not
materialization replacements. Materialization provenance belongs to L6H/L6I.

## Error And Recovery Facts

Add typed error/outcome variants for:

1. invalid compaction request;
2. non-compactable level;
3. stale candidate;
4. invalid input table descriptor;
5. unsafe retention request;
6. L5 compaction failure;
7. output artifact decode failure;
8. output level invariant violation;
9. release-plan disagreement.

Outcome/recovery facts should distinguish:

1. no-op: no candidate;
2. built but not installed because preflight failed;
3. stale candidate: retry with fresh branch state;
4. installed replacement tables;
5. installed with releasable old refs;
6. installed with protected old refs.

No error or debug string may include row value bytes.

## Target Module Shape

Expected production layout after L6J:

```text
crates/storage-next/src/branch/
  mod.rs
  config.rs
  error.rs
  facts.rs
  identity.rs
  read.rs
  state.rs
  compaction.rs   # optional if state.rs grows too large
  tests.rs
```

Keep exported surface `pub(crate)`. If `compaction.rs` is added, `mod.rs`
should expose it only inside the crate and source guards must include it.

## Implementation Steps

### L6J-A: Source Map And Vocabulary

1. Add this plan and test plan.
2. Add porting-log entry before production code changes.
3. Add branch compaction request, candidate, retention policy, outcome, and
   recovery/error vocabulary.
4. Extend source guards to allow L6J-owned compaction entrypoints while still
   rejecting backend, service publication, lifecycle, old storage, and product
   DTO APIs.

### L6J-B: Candidate Selection

1. Implement deterministic branch-owned candidate selection.
2. Cover L0, L0-to-L1, and nonzero-level compaction shapes.
3. Validate branch id, level bounds, table refs, key ranges, and overlap facts.
4. Return typed no-op outcomes for non-compactable requests.

### L6J-C: Keep-All Compaction Build

1. Convert selected `BranchOwnedTable` readers into L5
   `TableCompactionSource`s.
2. Run L5 `TableCompactor` with keep-all policy.
3. Decode all output artifacts and build branch-owned descriptors.
4. Reject build/decode failures before state mutation.

### L6J-D: Atomic Install

1. Revalidate candidate refs against current branch state.
2. Replace selected inputs with outputs atomically.
3. Preserve unselected L0 and non-overlapping lower-level tables.
4. Preserve pinned views.
5. Emit outcome and release facts.

### L6J-E: Retention Proof Rejection Or Implementation

1. Define proof-backed pruning request shape.
2. Initially reject unsafe old-version, tombstone, and TTL pruning requests with
   typed errors if full proof enforcement is not implemented.
3. If proof enforcement is implemented in L6J, add policy tests for every drop
   reason and every resurrection hazard.

### L6J-F: Generated Coverage

1. Extend `BranchLsmScaffoldOutcome` with compaction counters.
2. Add generated scripts for L0/L1+ candidates, keep-all read parity,
   stale-candidate rejection, protected release facts, and unsafe pruning
   rejection.
3. Ensure `branch_lsm_properties.rs` requires every L6J counter to be nonzero.

### L6J-G: Closeout

1. Update the parent L6 plans and porting log.
2. Run the verification commands from the test plan.
3. Record deferred items for L8 scheduling, durable publication, physical
   cleanup, and any proof-backed pruning not shipped in this slice.

## Deferred

1. Background compaction scheduling: L8.
2. Manifest publication and crash recovery: L8.
3. Physical table object deletion/quarantine: L8/L4.
4. Durable retention proof generation: L8/L9 unless a local proof is already
   available.
5. Cross-branch compaction: not V1.
6. Inherited-table compaction before materialization: not L6J; materialize
   first.
7. Object-store fencing and conditional publish: L4/L8.

## Verification

Run at least:

```bash
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

If L6J changes L5 compaction behavior, also run:

```bash
cargo test -p strata-storage-next --locked --lib table::tests::compaction
cargo test -p strata-storage-next --locked --test table_runtime_properties
```

## Exit Criteria

L6J is complete when:

1. branch-owned compaction candidates are deterministic and validated;
2. keep-all branch compaction preserves all supported reads;
3. unsafe old-version, tombstone, and TTL pruning requests are either safely
   implemented behind proofs or rejected before mutation;
4. output install is atomic from the perspective of new read views;
5. pinned old read views remain valid;
6. old table release facts are emitted only after replacement reachability is
   visible;
7. generated tests exercise nonzero compaction counters;
8. source guards prove L6J did not import scheduler, backend, lifecycle,
   durable publication, old storage, product DTO, or wall-clock behavior;
9. the porting log records preserved old compaction behavior, intentional V1
   changes, deferred durable cleanup, and sensitivity probes.
