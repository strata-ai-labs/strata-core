# L6M Implementation Plan: Assurance Depth

Status: implemented

Parent plans:

1. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L6/l6l-l6-conformance-closeout-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L6/l6l-l6-conformance-closeout-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`

## Goal

Close the remaining L6 assurance-depth gap.

L6M is not a new branch-runtime feature slice. L6A through L6L delivered the
branch-isolated LSM runtime and the first conformance closeout. L6M makes that
closeout reference-grade by adding an independent inheritance-aware oracle,
real opcode-driven generated and fuzz contracts, and a concrete sensitivity
probe ledger.

L6M must answer these questions:

1. Can generated tests detect a correlated bug in fork gates, inherited key
   rewriting, child-local shadowing, or materialization?
2. Do fuzz targets exercise different branch-runtime surfaces, or do they only
   dispatch fixed scenario shells?
3. Do range scans, history, timestamp reads, and materialization compare
   production results against an independently implemented model?
4. Can a reviewer map every required L6 semantic sensitivity probe to a test or
   source guard that fails?
5. Is any remaining L6 test-plan item either implemented or explicitly deferred
   to the correct upper layer?

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/storage/l5-table-runtime.md`
3. `docs/architecture/storage/commit-timeline-substrate.md`
4. `docs/architecture/storage/implementation-patterns.md`
5. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/l6l-l6-conformance-closeout-test-plan.md`
7. `crates/storage-next/src/branch/`
8. `crates/storage-next/src/testkit/branch_lsm.rs` and
   `crates/storage-next/src/testkit/branch_lsm/`
9. `crates/storage-next/tests/branch_lsm_properties.rs`
10. `crates/storage-next/tests/branch_lsm_closeout.rs`
11. `crates/storage-next/tests/branch_lsm_source_guard.rs`
12. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_reads.rs`
13. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_inheritance.rs`
14. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_install.rs`
15. `crates/storage-next/fuzz/corpus/branch_lsm_reads/`
16. `crates/storage-next/fuzz/corpus/branch_lsm_inheritance/`
17. `crates/storage-next/fuzz/corpus/branch_lsm_install/`

## Current Gaps

The L6 runtime implementation is not the main risk. The remaining risk is that
the broad tests still compare too many branch semantics against hand-built
fixtures or production-derived expectations.

L6M addresses these gaps:

1. `ModelBranch` currently models own-branch rows only. It does not model
   inherited layers, fork-version gates, key rewriting, child-local shadowing,
   chained ancestry, materialization, or range scans.
2. The generated property route exercises many L6 counters, but inheritance and
   materialization expectations are still partly hand-authored in the same
   helper functions that construct production states.
3. `branch_lsm_inheritance` and `branch_lsm_install` call dedicated contracts,
   but those contracts still rely heavily on fixed scenarios parameterized by
   a small number of script bytes.
4. Fuzz corpora exist, but they should include explicit operation-script seeds
   for fork/shadow/materialize/snapshot/compaction paths after the opcode
   decoder lands.
5. The sensitivity-probe section in the porting log records categories, not a
   per-probe mutation site and fired test or guard.
6. The remaining fork-reachability-before-visibility fault-window requirement
   needs either an L6-local test or an explicit L8 deferral.

## Scope

L6M may change:

1. `crates/storage-next/src/testkit/branch_lsm.rs` and its split module files
   under `crates/storage-next/src/testkit/branch_lsm/`;
2. `crates/storage-next/tests/branch_lsm_properties.rs`;
3. `crates/storage-next/tests/branch_lsm_closeout.rs`;
4. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_*.rs`;
5. `crates/storage-next/fuzz/corpus/branch_lsm_*/*`;
6. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`;
7. L6M implementation and test-plan docs.

L6M may change production `crates/storage-next/src/branch/` only when the new
oracle exposes a real implementation bug. Production changes must stay local to
the branch runtime and must not add a new feature surface.

## Non-Goals

L6M must not implement:

1. commit-version allocation;
2. commit conflict validation;
3. WAL-before-visible discipline;
4. branch registry workflows;
5. branch clear/delete APIs;
6. fork-at-history APIs;
7. durable materialization recovery records;
8. backend fault injection or durable publish reconciliation;
9. compaction scheduling;
10. retention scheduling;
11. query planning or secondary indexes;
12. StrataHub remote refs, push, pull, clone, or sync;
13. product `VersionedValue`, `Value`, `Key`, or branch DTO mapping.

If an audit item requires one of these responsibilities, L6M records an
explicit deferral with owner layer and reason.

## Target Model

### Independent Oracle Shape

Add an independent model under the `crates/storage-next/src/testkit/branch_lsm`
module. The exact names may change, but the responsibilities should be:

```text
ModelBranchStore
  branches: BranchId -> ModelBranch

ModelBranch
  branch_id
  own_rows: Vec<StorageRow>
  inherited_layers: Vec<ModelInheritedLayer> nearest-first

ModelInheritedLayer
  source_branch_id
  fork_version
  rows: Vec<StorageRow> in source branch keyspace
  status: active/materializing/materialized when relevant

ModelVisibleRow
  row: StorageRow in child branch keyspace
  source: own or inherited source facts
```

The model must not call production `BranchReadView` to compute expected
results. It may use low-level row constructors and stable encoding helpers, but
the visibility algorithm must be independent:

1. collect own rows for the target branch;
2. collect inherited rows from nearest layer to farthest layer;
3. skip inherited rows above that layer's fork version;
4. rewrite inherited rows from source branch id to child branch id;
5. group candidates by physical key;
6. apply read bound;
7. sort by commit version descending;
8. break exact-version ties by source precedence:
   child-local state first, then nearer inherited layer;
9. apply tombstone and TTL visibility using the requested timestamp only;
10. return point, history, prefix scan, and range scan results.

### Model Operations

The model should support the same bounded operation set used by generated and
fuzz tests:

1. create branch;
2. append put;
3. append tombstone;
4. append expiring put;
5. rotate active;
6. install L0 table;
7. fork branch;
8. append child-local put after fork;
9. append child-local tombstone after fork;
10. materialize inherited layer;
11. compact owned immutable tables when the production operation is available;
12. snapshot install into one or more branches;
13. point read latest;
14. point read at version;
15. point read as of timestamp;
16. history read;
17. prefix scan;
18. range scan;
19. capture pinned view, mutate state, then read through the pinned view.

The operation set should stay bounded and deterministic:

1. maximum 4 model branches;
2. maximum 8 logical keys;
3. maximum 64 operations per script;
4. small values;
5. bounded inherited depth;
6. no backend IO;
7. no product DTOs.

## Implementation Steps

### L6M-A: Inventory And Baseline

1. Add an `L6M` section to `m4-l6-porting-log.md`.
2. Record the current assurance gaps and the intended closeout criteria.
3. Run the current branch conformance commands to establish a baseline.

Exit: the porting log has an L6M entry before testkit behavior changes.

### L6M-B: Replace Own-Only Model With Branch Store Model

1. Refactor the existing `ModelBranch` into a model branch store.
2. Preserve the existing own-branch operation-script coverage.
3. Add independent point, history, prefix scan, and range scan expected-result
   functions.
4. Keep the model deliberately simple and slow; correctness matters more than
   speed.

Exit: existing `branch_lsm_properties.rs` passes with the new model and still
checks own-branch latest/getv/as-of/history/scan behavior.

### L6M-C: Add Inheritance Semantics To The Model

1. Add model inherited layers with source branch id and fork version.
2. Implement source-to-child branch-id rewriting in the model.
3. Implement fork-version gates in the model.
4. Implement child-local put and tombstone shadowing.
5. Implement chained ancestry ordering.
6. Compare production inherited point reads, history reads, prefix scans, and
   range scans against the model.

Exit: generated properties fail if inherited rows are not rewritten, if fork
gates are omitted, or if child-local rows do not outrank inherited rows.

### L6M-D: Add Materialization Parity To The Model

1. Model materialization as moving inherited visible rows into child-owned
   replacement rows without changing logical read results.
2. Compare production reads before materialization, model reads before
   materialization, production reads after materialization, and model reads
   after materialization.
3. Cover point, history, prefix scan, range scan, tombstone, TTL, and
   same-internal-key collision cases.
4. Keep durable recovery and backend fault windows deferred to L8.

Exit: materialization generated checks no longer rely only on production
before/after parity.

### L6M-E: Add Opcode Decoder For Generated Contracts

1. Introduce a deterministic script decoder for branch operation sequences.
2. Use the decoder from `check_branch_lsm_reference_model_contract`.
3. Add model-backed variants for inheritance and install flows instead of
   dispatching only fixed scenarios.
4. Track counters for each modeled category:
   - own latest/getv/as-of/history;
   - own prefix/range scans;
   - inherited point/history/prefix/range scans;
   - fork gates;
   - branch-id rewrites;
   - child put shadows;
   - child tombstone shadows;
   - materialization point/history/scan parity;
   - snapshot install model parity;
   - compaction install model parity.

Exit: `branch_lsm_properties.rs` requires the new counters and fails when a
counter is not exercised.

### L6M-F: Deepen Fuzz Contracts

Keep the three existing fuzz targets, but make each one drive a distinct
operation family:

1. `branch_lsm_reads`
   - owns own-branch operation scripts;
   - stresses latest/getv/as-of/history/prefix/range;
   - includes tombstones and TTL.
2. `branch_lsm_inheritance`
   - owns fork, chained fork, inherited rewrite, shadowing, timestamp inherited
     reads, and materialization parity scripts.
3. `branch_lsm_install`
   - owns L0 install, compaction install, snapshot install, identity collision,
     and reachability/release scripts.

Each fuzz target must pass bytes directly into its dedicated contract and must
not fall back to the broad scaffold contract as its primary route.

Exit: closeout inventory proves each branch fuzz target calls its own contract,
and each contract decodes enough bytes to drive multiple operations.

### L6M-G: Refresh Fuzz Seed Corpora

Add small, checked-in seed scripts for:

1. own put/tombstone/history/range;
2. TTL as-of read;
3. fork and inherited read;
4. child put shadowing inherited put;
5. child tombstone shadowing inherited put;
6. chained fork;
7. materialization parity;
8. snapshot multi-branch install;
9. compaction install;
10. identity collision rejection.

Exit: `branch_lsm_closeout.rs` verifies every branch fuzz corpus has more than
one non-empty seed and at least one named human-readable scenario seed.

### L6M-H: Sensitivity Probe Ledger

Replace category-level sensitivity text in `m4-l6-porting-log.md` with a table.

Each row must include:

1. probe id;
2. mutation description;
3. intended mutation site;
4. expected failing test, property, fuzz contract, or source guard;
5. command used;
6. status: run, structurally enforced, or deferred with owner layer.

Required probes:

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
15. expose old `VersionedValue` or product `Value` in production branch code;
16. import `crate::commit`, `crate::lifecycle`, `crate::backend`, or
   `crate::service` in production branch code.

Exit: the porting log gives a reviewer a concrete mutation-to-failure map.

### L6M-I: Resolve Remaining Fault-Window Classification

For fork-reachability-before-visibility:

1. add an L6-local test if the transition can be simulated without durable IO;
2. otherwise add an explicit L8 deferral row explaining that the window is a
   durable orchestration concern.

Exit: no L6 test-plan fault-window requirement remains ambiguous.

### L6M-J: Closeout Verification

Run:

```bash
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo test -p strata-storage-next --locked --test branch_lsm_closeout
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

When nightly fuzzing is available, run short smoke commands:

```bash
cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_reads -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_inheritance -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_install -- -max_total_time=60
```

Exit: mandatory commands pass, fuzz smoke either passes or is explicitly noted
as unavailable, and the porting log records the command results.

## Expected Files

Likely changed files:

1. `crates/storage-next/src/testkit/branch_lsm.rs`
2. `crates/storage-next/src/testkit/branch_lsm/scaffold.rs`
3. `crates/storage-next/tests/branch_lsm_properties.rs`
4. `crates/storage-next/tests/branch_lsm_closeout.rs`
5. `crates/storage-next/fuzz/.gitignore`
6. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_reads.rs`
7. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_inheritance.rs`
8. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_install.rs`
9. `crates/storage-next/fuzz/corpus/branch_lsm_reads/*`
10. `crates/storage-next/fuzz/corpus/branch_lsm_inheritance/*`
11. `crates/storage-next/fuzz/corpus/branch_lsm_install/*`
12. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
13. `docs/architecture/implementation-plans/M4/L6/l6m-assurance-depth-implementation-plan.md`
14. `docs/architecture/implementation-plans/M4/L6/l6m-assurance-depth-test-plan.md`

Production branch files should not change unless the new oracle identifies a
real bug.

## Exit Gate

L6M is complete when:

1. generated model tests cover own and inherited latest/getv/as-of/history,
   prefix scans, and range scans;
2. generated model tests cover fork gates, inherited branch-id rewriting,
   child-local put shadowing, child-local tombstone shadowing, chained ancestry,
   and materialization parity;
3. install-oriented generated tests compare snapshot and compaction install
   behavior against the model or a separately implemented install oracle;
4. fuzz contracts decode operation scripts and exercise distinct branch
   surfaces;
5. branch fuzz corpora include multiple non-empty seeds and named scenario
   seeds;
6. `branch_lsm_properties.rs` requires the new model-backed counters;
7. `branch_lsm_closeout.rs` verifies the strengthened fuzz and counter
   inventory;
8. `m4-l6-porting-log.md` contains a per-probe sensitivity ledger;
9. fork-reachability-before-visibility is either tested locally or explicitly
   deferred to L8;
10. all mandatory verification commands pass.

After L6M, L6 can be treated as closed for runtime and assurance purposes. Any
remaining branch behavior should be owned by L7 commit runtime, L8 lifecycle,
L9 public API mapping, or post-V1 retained-history/query work.
