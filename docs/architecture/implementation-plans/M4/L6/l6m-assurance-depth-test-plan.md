# L6M Test Plan: Assurance Depth

Status: implemented

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6m-assurance-depth-implementation-plan.md`

## Goal

Prove that L6 closeout has real assurance depth, especially for inheritance,
fork-version gates, branch-id rewriting, materialization, snapshot install, and
branch compaction. L6M is a testkit and evidence slice; it does not add a new
branch-runtime feature.

The suite must fail if:

1. inheritance or install checks compare production behavior only to
   production-derived expectations;
2. fork-version gates, inherited branch-id rewriting, or child-local shadowing
   regress;
3. materialization changes latest, versioned, timestamp, history, prefix, or
   range reads;
4. snapshot install or branch compaction stops matching an independent model;
5. branch fuzz targets collapse back to one broad scaffold route;
6. branch fuzz corpora stop containing scenario seeds;
7. sensitivity-probe evidence becomes only category-level prose;
8. a remaining L6 fault-window item is neither tested nor explicitly deferred.

## Test Locations

Use these locations:

1. `crates/storage-next/src/testkit/branch_lsm.rs`;
2. `crates/storage-next/src/testkit/branch_lsm/scaffold.rs`;
3. `crates/storage-next/tests/branch_lsm_properties.rs`;
4. `crates/storage-next/tests/branch_lsm_closeout.rs`;
5. `crates/storage-next/tests/branch_lsm_source_guard.rs`;
6. `crates/storage-next/fuzz/.gitignore`;
7. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_reads.rs`;
8. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_inheritance.rs`;
9. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_install.rs`;
10. `crates/storage-next/fuzz/corpus/branch_lsm_reads/`;
11. `crates/storage-next/fuzz/corpus/branch_lsm_inheritance/`;
12. `crates/storage-next/fuzz/corpus/branch_lsm_install/`;
13. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`.

## Model Oracle

The `crates/storage-next/src/testkit/branch_lsm` module must contain an
independent branch-store model with these responsibilities:

1. own-branch rows;
2. nearest-first inherited layers;
3. source branch id and fork-version facts per inherited layer;
4. inherited source-to-child branch-id rewriting;
5. fork-version filtering before visibility;
6. child-local precedence over inherited candidates;
7. tombstone filtering;
8. TTL filtering only when the caller supplies an as-of timestamp;
9. latest point reads;
10. version-bounded point reads;
11. timestamp-bounded point reads;
12. history reads;
13. prefix scans;
14. range scans.

Required model symbols:

1. `ModelBranchStore`;
2. `ModelInheritedLayer`;
3. `assert_model_store_read_surface`;
4. `check_branch_lsm_inheritance_model_contract`;
5. `check_branch_lsm_install_model_contract`.

The model may reuse low-level row construction and stable key-encoding helpers,
but it must not call `BranchReadView` or production branch candidate selection
to compute expected read results.

## Generated Properties

`branch_lsm_properties.rs` must require the model-backed contracts and model
symbols so a future removal is visible in normal test runs.

Required commands:

```bash
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
```

Required model-backed surfaces:

1. own-branch read model;
2. inherited layer read model;
3. chained inherited layers;
4. child-local put shadowing;
5. child-local tombstone shadowing;
6. materialization before/after read parity against the model;
7. snapshot install into multiple branches against the model;
8. compaction install against the model.

## Closeout Guards

`branch_lsm_closeout.rs` must enforce:

1. all generated counter methods are asserted by the property harness;
2. the model-backed contract names remain present;
3. the independent model names remain present;
4. branch fuzz targets are registered;
5. branch fuzz targets call their dedicated contracts;
6. branch fuzz targets do not call only the broad scaffold contract;
7. each branch fuzz corpus has at least two non-empty seeds;
8. each branch fuzz corpus has at least one human-readable scenario seed whose
   filename contains `script`.

The porting log sensitivity ledger is human-reviewed closeout evidence. It is
not enforced by a unit test so runtime tests stay focused on implementation
behavior rather than document path trivia.

Required command:

```bash
cargo test -p strata-storage-next --locked --test branch_lsm_closeout
```

## Fuzz Targets

The branch fuzz targets remain:

1. `branch_lsm_reads`;
2. `branch_lsm_inheritance`;
3. `branch_lsm_install`.

Each target must call its dedicated contract:

1. `check_branch_lsm_reads_contract`;
2. `check_branch_lsm_inheritance_contract`;
3. `check_branch_lsm_install_contract`.

Short smoke commands when nightly fuzzing is available:

```bash
cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_reads -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_inheritance -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_install -- -max_total_time=60
```

Fuzz smoke is optional for ordinary local closeout because it requires the
nightly fuzz toolchain. Target registration, dedicated-contract calls, and
corpus shape are mandatory and enforced by `branch_lsm_closeout.rs`.

## Sensitivity Ledger

`m4-l6-porting-log.md` must include a per-probe table with:

1. probe id;
2. mutation;
3. mutation site;
4. expected failing test, property, fuzz contract, or source guard;
5. status.

Required probe categories:

1. sort commit versions ascending instead of descending;
2. ignore tombstones;
3. evaluate TTL against wall clock instead of requested timestamp;
4. omit inherited fork-version gate;
5. skip inherited branch-id rewrite;
6. read inherited state before child-local state;
7. let child tombstones fall through to inherited puts;
8. remove an inherited layer before replacements are visible;
9. release tables still referenced by another branch or inherited layer;
10. let compaction drop old versions without proof;
11. let compaction drop tombstones without proof;
12. accept snapshot branch-id mismatch;
13. accept duplicate snapshot internal keys;
14. mutate one snapshot target before later validation fails;
15. expose old product `VersionedValue`/`Value`/`Key` vocabulary;
16. import forbidden lower/upper layers in production L6 code.

## Fault Window

Fork-reachability-before-visibility is not an L6-local durable IO transition.
If it remains untestable without L8 orchestration, the porting log must defer
it explicitly to L8 rather than leave it as an open L6 test gap.

## Full Verification

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

L6M is complete when the mandatory commands pass and fuzz smoke is either run
successfully or recorded as unavailable for the local toolchain.
