# L6L Implementation Plan: L6 Conformance Closeout

Status: draft implementation plan

Parent plans:

1. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L6/l6a-branch-runtime-scaffold-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L6/l6b-branch-row-identity-read-bounds-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/l6c-branch-local-mutable-frozen-state-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L6/l6d-pinned-own-branch-read-views-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-implementation-plan.md`
10. `docs/architecture/implementation-plans/M4/L6/l6g-timestamp-ttl-visibility-implementation-plan.md`
11. `docs/architecture/implementation-plans/M4/L6/l6h-materialization-mechanics-implementation-plan.md`
12. `docs/architecture/implementation-plans/M4/L6/l6i-reachability-shared-table-refs-implementation-plan.md`
13. `docs/architecture/implementation-plans/M4/L6/l6j-branch-compaction-integration-implementation-plan.md`
14. `docs/architecture/implementation-plans/M4/L6/l6k-snapshot-row-install-implementation-plan.md`
15. `docs/architecture/implementation-plans/M4/L6/l6l-l6-conformance-closeout-test-plan.md`

## Goal

Close M4-L6 as a coherent branch-isolated LSM runtime.

L6L is not a new branch feature slice. It is the conformance, audit,
documentation, fuzz-inventory, and hardening pass that proves L6A through L6K
compose correctly and are ready for L7 commit runtime and L8 lifecycle
orchestration.

L6L must answer these questions with code, tests, or explicit deferrals:

1. Does `crates/storage-next/src/branch/` stay pure L6?
2. Do all branch mechanics use storage-next `StorageRow`, `BranchId`,
   `CommitVersion`, `Timestamp`, and L5 table facts only?
3. Are branch-local state, read views, inherited layers, timestamp/TTL reads,
   materialization, reachability, compaction install, and snapshot row install
   covered by direct examples and generated models?
4. Do generated counters prove every L6 category is exercised?
5. Do fuzz targets hit the branch read, inheritance/materialization, and install
   surfaces through dedicated testkit contracts?
6. Does every old `crates/storage` branch/LSM behavior have an entry in the
   porting log as preserved, rewritten, retired, or deferred?
7. Can L7 and L8 run a stable command set and trust L6 as a lower layer?

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/storage/l5-table-runtime.md`
3. `docs/architecture/storage/commit-timeline-substrate.md`
4. `docs/architecture/storage/implementation-patterns.md`
5. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
6. `docs/spec/strata-storage-format-v1.md`
7. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
8. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
9. all L6A through L6K implementation and test plans
10. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
11. `crates/storage-next/src/branch/`
12. `crates/storage-next/src/testkit/branch_lsm.rs`
13. `crates/storage-next/tests/branch_lsm_properties.rs`
14. `crates/storage-next/tests/branch_lsm_source_guard.rs`
15. `crates/storage-next/fuzz/`
16. relevant old-code evidence under `crates/storage/src/segmented/`,
    `crates/storage/src/memtable.rs`, `crates/storage/src/merge_iter.rs`,
    `crates/storage/src/seekable.rs`, and
    `crates/storage/src/durability/decoded_snapshot_install.rs`

## Scope

L6L implements closeout work only:

1. conformance inventory for every L6 module and every L6A-L6K exit gate;
2. generated branch-LSM harness completeness checks;
3. source-boundary guard consolidation for L6 purity;
4. branch-LSM fuzz target inventory and seed corpora;
5. testkit contract functions dedicated to the fuzz targets;
6. closeout integration tests that verify counters, source guards, and fuzz
   inventory;
7. command-level conformance documentation for default, no-default, wasm,
   all-features, and fuzz-smoke modes;
8. porting-log closeout entries for behavior preserved, intentionally changed,
   retired, or deferred;
9. sensitivity-probe ledger for L6 semantics;
10. small test holes or assertion gaps found during the closeout audit;
11. final M4-L6 exit-gate checklist.

L6L may add small helper functions, source guards, testkit counters, fuzz
targets, corpora, or tests. It should not add a new production branch subsystem
unless the closeout audit proves an earlier L6 slice left an incomplete
contract.

## Non-Goals

L6L must not implement:

1. commit-version allocation;
2. commit conflict validation;
3. WAL-before-visible discipline;
4. branch manifest publication;
5. durable checkpoint or recovery orchestration;
6. durable compaction scheduling;
7. durable snapshot publication or decode orchestration;
8. retention scheduling;
9. quarantine or repair orchestration;
10. product branch merge, cherry-pick, revert, review, or restore semantics;
11. public branch API stabilization;
12. StrataHub push, pull, clone, sync, or remote-tracking behavior;
13. query planning or secondary indexing;
14. `VersionedValue` or product DTO mapping.

If a gap belongs to one of these areas, L6L records it as an L7, L8, L9, or
post-V1 deferral and adds a guard only when the current L6 boundary can
regress accidentally.

## Current Surface To Close

| Surface | Files | L6L question |
|---|---|---|
| branch scaffold | `src/branch/{mod.rs,error.rs,config.rs,facts.rs}` | Are exports crate-private, typed, and sufficient for L7/L8 without product leakage? |
| row identity/read bounds | `src/branch/identity.rs`, `read.rs` | Are branch id validation, key rewriting, and read bounds model-checked? |
| mutable/frozen state | `src/branch/state.rs` | Are append, tombstone, rotation, facts, and pinned view behavior covered? |
| read views | `src/branch/read.rs` | Do latest/getv/as-of/history/prefix/range reads match row-chain models? |
| immutable levels | `src/branch/state.rs` and L5 readers | Are L0/L1+ ownership and read ordering covered without backend imports? |
| inherited layers | `src/branch/state.rs`, `read.rs` | Are fork gates, key rewriting, ancestry order, and child-local shadowing covered? |
| timestamp/TTL | `src/branch/read.rs` | Are timestamp facts, TTL boundaries, and insufficient-history behavior covered? |
| materialization | `src/branch/state.rs` | Does physical ownership change without changing reads or pinned views? |
| reachability | `src/branch/reachability` facts in branch state | Are shared table refs deterministic and safe to release only when unreachable? |
| compaction integration | branch compaction helpers | Does L6 choose candidates and install outputs without scheduling or unsafe pruning? |
| snapshot install | branch snapshot install helpers | Is row-native multi-branch install all-or-nothing and DTO-free? |
| generated harness | `src/testkit/branch_lsm.rs` | Does one bounded route exercise every L6 category? |
| external guards | `tests/branch_lsm_*.rs` | Do source scans and properties fail on L6 regressions? |
| fuzz inventory | `fuzz/fuzz_targets/branch_lsm_*.rs` | Do fuzz targets call dedicated contracts and carry seed corpora? |

## Closeout Rules

1. Prefer strengthening tests over changing production code.
2. Keep new production changes mechanical and local.
3. Do not move commit, lifecycle, backend, or service logic into
   `src/branch/`.
4. Do not add product value, branch-name, dataset, StrataHub, or query
   semantics.
5. Do not expose new public APIs outside the existing hidden testkit surface.
6. Do not weaken source guards to pass closeout.
7. Do not rely on old `crates/storage` tests as proof.
8. Every newly found gap is either fixed in L6L or recorded with an owner layer.
9. Every closeout claim must point to a test, guard, fuzz target, command, or
   explicit deferral.

## Implementation Steps

### L6L-A: Inventory Existing Coverage

Build a coverage matrix from current code and tests.

Rows:

1. L6A scaffold/config/facts/errors;
2. L6B row identity and read bounds;
3. L6C mutable/frozen state;
4. L6D pinned own-branch reads;
5. L6E branch-owned immutable levels;
6. L6F fork and inherited layers;
7. L6G timestamp/TTL visibility;
8. L6H materialization mechanics;
9. L6I reachability and shared table refs;
10. L6J branch compaction integration;
11. L6K snapshot row install.

Columns:

1. direct unit tests;
2. generated/property counters;
3. source guard coverage;
4. fuzz or fuzz-adjacent coverage;
5. cross-feature coverage;
6. old-code behavior mapped;
7. deferred behavior and owner;
8. mandatory commands.

Output:

1. an L6L section in `m4-l6-porting-log.md`;
2. missing test/doc/source-guard/fuzz items to close in later L6L steps.

### L6L-B: Strengthen Source Guards

Review `tests/branch_lsm_source_guard.rs` and add probes only where gaps
remain.

Guard categories:

1. upper-layer imports: `crate::commit`, `crate::lifecycle`, `crate::api`;
2. engine crate imports;
3. product DTO vocabulary: `VersionedValue`, `Versioned`, old `Value`,
   old `Key`, `Namespace`, `TypeTag`, `EntityRef`, `TransactionContext`;
4. StrataHub, remote, dataset, user workflow, and product branch-name
   vocabulary;
5. filesystem/path/backend usage in production `src/branch/`;
6. L4 service calls and object layout literals in production `src/branch/`;
7. WAL/checkpoint/recovery scheduling calls;
8. wall-clock/environment reads;
9. bare public API leaks;
10. testkit imports from production branch code;
11. premature L7/L8/L9 entrypoints.

The guard must distinguish production branch code from tests, testkit, and
docs. L6 can hold row/table facts; it cannot own durable IO or product
semantics.

### L6L-C: Consolidate Generated Harness Counters

Audit `BranchLsmScaffoldOutcome`.

Every L6 category must have:

1. a counter;
2. a nonzero assertion in `tests/branch_lsm_properties.rs`;
3. at least one generated check that can fail independently;
4. direct or slice-level coverage in branch tests;
5. a matching row in the porting log closeout matrix.

Expected generated categories:

1. config/facts/errors/stats;
2. row identity and branch rewrite;
3. effective read bounds;
4. active/frozen append/rotation/facts;
5. pinned own-branch latest/getv/history/prefix/range reads;
6. timestamp/as-of reads and TTL boundaries;
7. immutable table install/read ordering;
8. fork and inherited layers;
9. child-local shadowing;
10. materialization;
11. reachability/shared refs/release facts;
12. branch compaction candidate/install/safety;
13. snapshot row install;
14. invalid request rejection and no-mutation paths.

### L6L-D: Add Dedicated Branch-LSM Fuzz Contracts

Add hidden testkit functions for branch fuzz targets:

1. `check_branch_lsm_reads_contract(script: &[u8])`;
2. `check_branch_lsm_inheritance_contract(script: &[u8])`;
3. `check_branch_lsm_install_contract(script: &[u8])`.

The contracts should be narrower than `check_branch_lsm_scaffold_contract`.
They must drive the specific surface named by the fuzz target and must not all
delegate to the shared scaffold route.

Target focus:

1. `branch_lsm_reads`: latest/getv/as-of/history/prefix/range over generated
   own-branch and inherited row chains;
2. `branch_lsm_inheritance`: fork gates, key rewriting, ancestry order,
   child-local shadowing, timestamp inherited reads, and materialization parity;
3. `branch_lsm_install`: branch-owned table install, compaction install,
   reachability release facts, and snapshot row install invalid/valid plans.

### L6L-E: Add Branch-LSM Fuzz Targets And Corpora

Add libFuzzer targets under `crates/storage-next/fuzz/fuzz_targets/`:

1. `branch_lsm_reads.rs`;
2. `branch_lsm_inheritance.rs`;
3. `branch_lsm_install.rs`.

Register each target in `crates/storage-next/fuzz/Cargo.toml`.

Add checked-in seed corpora:

1. `crates/storage-next/fuzz/corpus/branch_lsm_reads/basic-script`;
2. `crates/storage-next/fuzz/corpus/branch_lsm_inheritance/fork-shadow-script`;
3. `crates/storage-next/fuzz/corpus/branch_lsm_install/snapshot-install-script`.

Each target must:

1. import exactly its dedicated contract function;
2. pass the fuzzer bytes directly to that function;
3. panic only on `TestkitError`;
4. avoid `check_branch_lsm_scaffold_contract` unless used as an additional
   smoke path outside the fuzz target.

### L6L-F: Add Closeout Inventory Tests

Add `crates/storage-next/tests/branch_lsm_closeout.rs`.

It should verify:

1. generated harness exposes every required L6 counter;
2. property test requires every counter to be nonzero;
3. source guard suite contains probes for every required boundary category;
4. fuzz targets exist and are registered in `fuzz/Cargo.toml`;
5. every branch fuzz target calls its dedicated contract function;
6. no branch fuzz target calls only the shared scaffold contract;
7. each branch fuzz target has a non-empty checked-in corpus directory;
8. porting log contains L6A through L6L sections;
9. mandatory command set is recorded in the parent plan or porting log;
10. deferred ledger names owner layers for remaining behavior.

### L6L-G: Fill Small Test Holes Found By Audit

Use the inventory to add focused tests when gaps are narrow.

Likely closeout additions:

1. exact source-guard probes for any missing forbidden terms;
2. direct tests for missing generated counter categories;
3. generated checks for no-mutation invalid paths that were only unit-tested;
4. closeout tests for branch fuzz inventory;
5. doc-only clarifications when behavior is intentionally deferred.

Do not broaden L6L into a new feature slice. If a gap requires durable IO,
commit allocation, lifecycle recovery, or public API work, record it as a
future owner-layer item.

### L6L-H: Reconcile Docs And Porting Log

Update:

1. `m4-l6-porting-log.md` with an L6L closeout section;
2. parent L6 implementation plan exit gate if any wording drifted from code;
3. parent L6 test plan command table and deferred behavior map;
4. slice docs only when they make stale claims about delivered coverage or
   deferrals.

The closeout section must list:

1. behavior preserved from current storage;
2. intentional storage-next changes;
3. behavior retired from L6;
4. behavior deferred to L7/L8/L9/post-V1;
5. sensitivity probes run or structurally enforced;
6. exact verification commands and results.

### L6L-I: Run Mandatory Verification

Run the mandatory L6 closeout command set:

```bash
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo test -p strata-storage-next --locked --test branch_lsm_closeout
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --locked
cargo fmt --package strata-storage-next --check
git diff --check
```

When nightly fuzzing is available, run short smoke fuzz commands:

```bash
cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_reads -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_inheritance -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_install -- -max_total_time=60
```

If nightly fuzzing is not available, the closeout inventory tests must still
verify target registration, dedicated contracts, and seed corpora.

## Files Expected To Change

Likely files:

1. `crates/storage-next/src/testkit/branch_lsm.rs`
2. `crates/storage-next/src/testkit/mod.rs`
3. `crates/storage-next/tests/branch_lsm_properties.rs`
4. `crates/storage-next/tests/branch_lsm_source_guard.rs`
5. `crates/storage-next/tests/branch_lsm_closeout.rs`
6. `crates/storage-next/fuzz/Cargo.toml`
7. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_reads.rs`
8. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_inheritance.rs`
9. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_install.rs`
10. `crates/storage-next/fuzz/corpus/branch_lsm_reads/*`
11. `crates/storage-next/fuzz/corpus/branch_lsm_inheritance/*`
12. `crates/storage-next/fuzz/corpus/branch_lsm_install/*`
13. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
14. parent L6 implementation/test plans if closeout clarifies exit gates

Production `crates/storage-next/src/branch/` should change only if L6L finds a
real correctness or API-contract gap.

## Exit Criteria

L6L is complete when:

1. L6A through L6K have a coverage matrix row with no unexplained blanks.
2. Generated properties assert every L6 counter is nonzero.
3. Branch source guards cover upper layers, product DTOs, backend/IO,
   service/layout/object leakage, wall-clock/env usage, and public leakage.
4. Three branch fuzz targets exist, are registered, call dedicated contracts,
   and have checked-in seed corpora.
5. `branch_lsm_closeout.rs` verifies the closeout inventory.
6. The porting log records preserved, changed, retired, and deferred behavior.
7. Mandatory verification commands pass or documented unavailable commands are
   replaced by structural checks.
8. Remaining work is assigned to L7, L8, L9, or post-V1, not left as an
   unnamed L6 gap.
