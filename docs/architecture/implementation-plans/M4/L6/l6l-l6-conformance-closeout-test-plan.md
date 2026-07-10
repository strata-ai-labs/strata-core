# L6L Test Plan: L6 Conformance Closeout

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6l-l6-conformance-closeout-implementation-plan.md`

## Goal

Prove that M4-L6 is complete as a branch-isolated LSM runtime and that L7/L8
can build on it without first repairing L6 tests, source boundaries, or
behavior documentation.

The suite must fail if:

1. any L6 slice lacks direct, generated, guard, or closeout coverage;
2. production `src/branch/` imports commit, lifecycle, engine, backend, object
   layout, or product semantics;
3. old `VersionedValue`, `Value`, `Key`, `Namespace`, `TypeTag`, or product
   branch names re-enter L6;
4. branch reads diverge from independent row-chain expectations;
5. fork gates, inherited key rewriting, or child-local shadowing regress;
6. materialization, compaction install, or snapshot install mutates state
   partially on invalid input;
7. reachability releases tables still referenced by branches or inherited
   layers;
8. generated branch-LSM counters stop exercising a category;
9. branch fuzz targets are missing, unregistered, lack seed corpora, or all call
   the same broad scaffold route;
10. the closeout docs claim a gap is closed without a test, command, guard,
    fuzz target, or explicit deferral.

## Test Locations

Use these locations:

1. `crates/storage-next/src/branch/tests.rs` for direct branch mechanics;
2. `crates/storage-next/src/testkit/branch_lsm.rs` for generated contracts;
3. `crates/storage-next/tests/branch_lsm_properties.rs` for property counters;
4. `crates/storage-next/tests/branch_lsm_source_guard.rs` for L6 boundaries;
5. `crates/storage-next/tests/branch_lsm_closeout.rs` for closeout inventory;
6. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_reads.rs`;
7. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_inheritance.rs`;
8. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_install.rs`;
9. `crates/storage-next/fuzz/corpus/branch_lsm_reads/`;
10. `crates/storage-next/fuzz/corpus/branch_lsm_inheritance/`;
11. `crates/storage-next/fuzz/corpus/branch_lsm_install/`;
12. `crates/storage-next/proptest-regressions/branch_lsm.txt`, created only
    when a failing generated seed is captured;
13. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`.

Tests must use storage-next `StorageRow`, `PhysicalKey`, `BranchId`,
`CommitVersion`, `Timestamp`, L5 table runtime, and L6 branch read/state facts.
Tests must not use old product DTOs, backend handles, object layout strings,
snapshot object names, product branch names, or StrataHub vocabulary.

## Coverage Matrix

L6L must produce or update a matrix with one row per L6 slice.

Required rows:

1. `L6A` scaffold/config/facts/errors;
2. `L6B` row identity and read bounds;
3. `L6C` mutable/frozen branch state;
4. `L6D` pinned own-branch read views;
5. `L6E` branch-owned immutable levels;
6. `L6F` fork and inherited layers;
7. `L6G` timestamp and TTL visibility;
8. `L6H` materialization mechanics;
9. `L6I` reachability and shared table refs;
10. `L6J` branch compaction integration;
11. `L6K` snapshot row install.

Required columns:

1. direct unit tests;
2. generated/property tests;
3. source guards;
4. fuzz or fuzz-adjacent coverage;
5. cross-feature coverage;
6. old-code behavior mapped;
7. deferred behavior and owner;
8. mandatory commands that exercise the row.

A blank cell is a test gap unless it has a named owner-layer deferral.

## Required Closeout Tests

### 1. Source Guard Completeness

The source guard suite must assert production `src/branch/` does not:

1. import `crate::commit`;
2. import `crate::lifecycle`;
3. import `crate::api`;
4. import engine crates;
5. import `crate::backend`;
6. import `crate::layout`;
7. import `crate::object`;
8. import `crate::service` or L4 durable service helpers;
9. call backend methods directly;
10. use `std::fs`, `Path`, `PathBuf`, `File`, `pread`, `rename`,
    `remove_file`, `mmap`, or platform-local filesystem APIs;
11. read environment variables;
12. read wall-clock time;
13. contain old product DTO vocabulary: `VersionedValue`, `Versioned`, old
    `Value`, old `Key`, `Namespace`, `TypeTag`, `EntityRef`,
    `TransactionContext`;
14. contain StrataHub, remote, dataset, provider, user workflow, or product
    branch-name vocabulary as behavior;
15. contain WAL append, checkpoint, recovery, retention, or quarantine
    orchestration calls;
16. expose bare public APIs;
17. import `crate::testkit`;
18. add behavior entrypoints that belong to L7, L8, L9, or engine-next.

The guard must include executable regression probes proving each forbidden
category is detected.

### 2. Generated Harness Counter Completeness

`BranchLsmScaffoldOutcome` must expose counters for every category:

1. valid/invalid configs;
2. read bounds;
3. valid/invalid facts;
4. descriptors;
5. error source chains;
6. stats;
7. row identity and rewrite;
8. effective bounds and row candidates;
9. row-chain and fork-edge facts;
10. branch-local append/rotation/frozen facts;
11. pinned read-view captures and isolation;
12. latest/getv/history/prefix/range reads;
13. timestamp/as-of reads and TTL boundaries;
14. immutable table install/read ordering;
15. inherited layer creation, validation, reads, scans, and shadowing;
16. materialization attempts, skipped rows, parity, and release facts;
17. reachability snapshots, aggregate rebuilds, registry updates, and release
    plans;
18. compaction no-ops, candidates, output install, safety rejections, parity,
    and release facts;
19. snapshot install no-op, valid install, invalid preflight, read parity,
    tombstone/TTL preservation, pinned views, reachability, and row boundary
    cases.

Tests must assert each counter is nonzero in:

1. `tests/branch_lsm_properties.rs`;
2. `tests/branch_lsm_closeout.rs` inventory checks, either directly or by
   scanning the property test.

Adding a new L6 category later requires adding a counter and nonzero assertion
in the same change.

### 3. Direct Test Completeness

Direct branch tests must cover:

1. branch runtime config, facts, descriptors, stats, and error source chains;
2. branch id validation and row branch mismatch rejection;
3. inherited row key rewriting;
4. committed put append;
5. committed tombstone append;
6. duplicate internal-key rejection without mutation;
7. active rotation and frozen ordering;
8. pinned read views across append and rotation;
9. latest, getv, timestamp, history, prefix, and range reads;
10. branch-owned L0 and L1+ immutable tables;
11. L1+ overlap rejection;
12. fork into an empty child without row copy;
13. parent post-fork invisibility;
14. child-local put and tombstone shadowing;
15. chained ancestry ordering;
16. timestamp inherited reads and TTL boundaries;
17. materialization preserving reads and pinned views;
18. materialization idempotency and invalid request rejection;
19. reachability snapshots and shared table registry rebuilds;
20. branch clear/delete release facts where implemented;
21. compaction candidate selection and keep-all parity;
22. unsafe old-version/tombstone/TTL pruning rejection;
23. compaction output install and release facts;
24. snapshot row install all-or-nothing behavior;
25. source/error strings do not leak row value bytes.

### 4. Generated Read Model Tests

Generated branch model tests must cover:

1. one physical key with many commit versions;
2. many physical keys in one branch;
3. active-only state;
4. frozen-only state;
5. immutable-only state;
6. mixed active/frozen/immutable state;
7. latest point reads;
8. version-bounded point reads;
9. timestamp-bounded point reads;
10. history including tombstones;
11. history excluding tombstones;
12. prefix scans;
13. range scans;
14. empty user keys;
15. embedded-zero and high-bit user keys;
16. multiple storage-space ids and names;
17. empty put values;
18. tombstones newest, middle, and older than puts;
19. TTL before, exactly at, and after expiry;
20. `Timestamp::EPOCH` and `Timestamp::MAX`;
21. non-monotonic timestamps relative to commit version;
22. pinned views after later mutation.

Every expected read result must come from an independent model, not from
production branch read helpers.

### 5. Generated Inheritance And Materialization Tests

Generated inheritance/materialization tests must cover:

1. one-level fork;
2. chained fork;
3. sibling forks sharing the same inherited tables;
4. fork-version gate for point reads;
5. fork-version gate for scans;
6. parent writes after fork invisible to child;
7. source inherited layers preserved in deterministic order;
8. inherited key rewrite before grouping;
9. child-local put shadows inherited put;
10. child-local tombstone shadows inherited put;
11. child-local row above requested version does not shadow a visible inherited
    row below the bound;
12. child-local row after requested timestamp does not shadow a visible
    inherited row at the timestamp bound;
13. nearest inherited layer wins exact tie cases;
14. materialization preserves latest reads;
15. materialization preserves getv reads;
16. materialization preserves as-of reads;
17. materialization preserves history;
18. materialization preserves prefix/range scans;
19. materialization skips only documented duplicate/post-fork rows;
20. materialization failure leaves old reads intact;
21. materialization expected reads are also checked against an independent
    model, not only by comparing production reads before and after
    materialization;
22. child-owned immutable rows with the same internal key as a materialized
    inherited row reject the materialization without mutation.

### 6. Generated Install And Transition Tests

Generated install/transition tests must cover:

1. branch-owned immutable table install;
2. invalid immutable install rejection without mutation;
3. compaction no-op;
4. L0 compaction candidate;
5. L0-to-L1 compaction candidate;
6. nonzero-level compaction candidate;
7. stale compaction plan rejection;
8. unsafe pruning policy rejection;
9. keep-all compaction read parity;
10. compaction output split;
11. compaction release and protected-release facts;
12. snapshot empty no-op;
13. snapshot single-branch install;
14. snapshot multi-branch install;
15. snapshot missing-branch reject/create policy;
16. snapshot non-empty target rejection;
17. snapshot empty/duplicate branch group rejection;
18. snapshot duplicate/unsorted row rejection;
19. snapshot branch mismatch rejection;
20. snapshot output identity collision rejection;
21. snapshot table-build failure atomicity;
22. snapshot read parity after install.

### 7. Closeout Inventory Test

Add `tests/branch_lsm_closeout.rs`.

It must verify:

1. `src/testkit/branch_lsm.rs` defines the required contract functions;
2. `src/testkit/mod.rs` exports the branch contract functions behind the
   hidden testkit surface;
3. `tests/branch_lsm_properties.rs` requires all generated counters;
4. `tests/branch_lsm_source_guard.rs` contains probes for every boundary
   category;
5. `fuzz/Cargo.toml` registers `branch_lsm_reads`,
   `branch_lsm_inheritance`, and `branch_lsm_install`;
6. each target file exists;
7. each target imports and calls its dedicated contract;
8. none of the branch fuzz targets uses only
   `check_branch_lsm_scaffold_contract`;
9. each target has a non-empty checked-in corpus directory;
10. the L6 porting log includes sections for L6A through L6L;
11. the closeout command set is recorded;
12. deferred behavior has explicit L7/L8/L9/post-V1 ownership.

### 8. Fuzz Target Structural Tests

Required branch fuzz targets:

1. `branch_lsm_reads`
   - calls `check_branch_lsm_reads_contract`;
   - focuses on read parity and row-chain visibility.
2. `branch_lsm_inheritance`
   - calls `check_branch_lsm_inheritance_contract`;
   - focuses on fork, key rewrite, shadowing, inherited timestamp reads, and
     materialization parity.
3. `branch_lsm_install`
   - calls `check_branch_lsm_install_contract`;
   - focuses on immutable table install, compaction install, reachability
     release facts, and snapshot row install.

Each target must have a small checked-in seed corpus. The seed corpus should be
human-readable byte scripts when practical, but correctness matters more than
readability.

### 9. Sensitivity Probes

Before closing L6, temporarily introduce each mutation and confirm a targeted
test or guard fails:

1. sort commit versions ascending inside one row chain;
2. ignore tombstones in latest reads;
3. evaluate TTL against wall-clock now instead of requested timestamp;
4. omit fork-version gate for inherited rows;
5. forget to rewrite inherited branch id;
6. search inherited layers before child-local state;
7. let child-local tombstones fall through to inherited puts;
8. remove inherited layer before materialized replacement is visible;
9. mark shared inherited tables releasable while another branch references
   them;
10. let compaction drop old versions without a retention proof;
11. let compaction drop tombstones without proving no resurrection;
12. accept snapshot row branch mismatch;
13. accept duplicate snapshot internal keys;
14. mutate one branch before later snapshot target validation fails;
15. expose old `VersionedValue` or `Value` in production branch code;
16. import `crate::commit`, `crate::lifecycle`, `crate::backend`, or
   `crate::service` in production branch code.

Record the sensitivity-probe outcomes in `m4-l6-porting-log.md`. When a probe
is structurally enforced by a source guard or closeout inventory test, record
that instead of keeping a local mutation.

## Cross-Feature Matrix

Mandatory modes:

| Mode | Purpose | Command |
|---|---|---|
| branch unit | fast branch mechanics check | `cargo test -p strata-storage-next --locked --lib branch` |
| source guards | L6 purity | `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard` |
| closeout inventory | generated/fuzz/doc inventory | `cargo test -p strata-storage-next --locked --test branch_lsm_closeout` |
| branch generated | generated model check | `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties` |
| no-default generated | prove no accidental localfs/default dependency | `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties` |
| wasm/no-default | browser-compatible branch mechanics | `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked` |
| lint | all-target/all-feature lint surface | `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` |
| full package | regression safety net | `cargo test -p strata-storage-next --locked` |
| format | rustfmt stability | `cargo fmt --package strata-storage-next --check` |
| whitespace | patch hygiene | `git diff --check` |

Fuzz smoke modes when nightly fuzzing is available:

```bash
cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_reads -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_inheritance -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_install -- -max_total_time=60
```

If nightly fuzzing is unavailable, `branch_lsm_closeout.rs` must still prove
target registration, dedicated contracts, and non-empty corpora.

## Deferred Behavior Map

Not L6 test gaps:

1. commit-version allocation belongs to L7;
2. commit conflict validation belongs to L7;
3. WAL-before-visible discipline belongs to L7/L8;
4. branch manifest publication belongs to L8;
5. durable recovery orchestration belongs to L8;
6. checkpoint scheduling belongs to L8;
7. compaction scheduling belongs to L8;
8. retention scheduling belongs to L8;
9. quarantine/repair orchestration belongs to L8;
10. branch-registry workflows such as duplicate branch create, fork on a
    missing source branch, and fork-at-history requests belong to L7/L9;
11. branch clear/delete APIs and pinned-view behavior across those public
    lifecycle operations belong to L8/L9;
12. public branch naming and product branch workflows belong to L9/engine;
13. product DTO mapping, including `Versioned<T>`, belongs to L9/engine;
14. materialization durability uncertainty, visible-but-not-durable publish
    windows, durable recovery records, and backend fault reconciliation belong
    to L8;
15. materialization provenance diagnostics across re-forked materialized
    replacement tables are deferred until L8 durable recovery facts define the
    persisted provenance shape;
16. StrataHub push/pull/clone/sync integration belongs above storage-next;
17. query planner and secondary indexes belong above the L6 branch LSM.

The porting log must name these deferrals if an audit might otherwise treat
them as L6 gaps.

## Exit Gate

M4-L6 test coverage is complete when:

1. direct tests cover every branch module surface;
2. generated model tests cover latest/getv/as-of/history/scans, fork,
   inheritance, timestamp/TTL, materialization, reachability, compaction
   install, and snapshot install;
3. `branch_lsm_closeout.rs` verifies generated counter, source guard, fuzz, and
   doc inventory;
4. branch fuzz targets exist with checked-in seed corpora and dedicated
   contracts;
5. source guards prevent commit/lifecycle/engine/product/backend/service/layout
   leakage;
6. sensitivity probes are run or structurally enforced and recorded;
7. parent plans and porting log identify old storage mechanics that were
   ported, rewritten, retired, or deferred;
8. all mandatory cross-feature commands pass.
