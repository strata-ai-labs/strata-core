# L7N Test Plan: L7 Conformance Closeout

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L7/l7n-l7-conformance-closeout-implementation-plan.md`

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`

## Goal

Prove that M4-L7 is closeable as a storage-layer commit runtime. The closeout
process should verify behavior, source boundaries, fuzz routing, corpus
coverage, fault coverage, generated counters, and command evidence.

It must not add tests that only prove planning documents exist, link to each
other, or contain a phrase. Documentation consistency is reviewed in the
porting log; automated tests stay focused on implementation assurance.

## Test Locations

Use these locations:

1. `crates/storage-next/tests/commit_runtime_closeout.rs` for closeout
   inventory.
2. `crates/storage-next/tests/commit_runtime_source_guard.rs` for source and
   boundary checks.
3. `crates/storage-next/tests/commit_runtime_fuzz_inventory.rs` for fuzz target
   and corpus checks.
4. `crates/storage-next/tests/commit_runtime_properties.rs` for generated
   model-vs-runtime coverage.
5. `crates/storage-next/tests/commit_runtime_faults.rs` for fault-window
   coverage.
6. `crates/storage-next/src/testkit/commit_runtime*.rs` for counters and
   contract entrypoints.
7. `crates/storage-next/fuzz/fuzz_targets/commit_runtime_*.rs` for fuzz target
   routing.
8. `crates/storage-next/fuzz/corpus/commit_runtime_*` for seed corpora.

## Fixture Rules

1. Keep closeout tests deterministic and filesystem-local to the repository.
2. Do not spawn threads, sleep, or depend on wall-clock time.
3. Do not mutate source files from tests.
4. Inspect source files only for implementation boundary rules.
5. Keep source scanning allowlists explicit and narrow.
6. Keep command evidence manual in the porting log.
7. Keep mutation probes out of the committed tree.

## Direct Test Matrix

### 1. Closeout Inventory

Add `commit_runtime_closeout.rs`.

Required cases:

1. generated assurance counters include every required category:
   cache, durable, replay, read-only, conflict, timeline, guard/quiesce, fault,
   visible-read parity, and model parity;
2. property tests assert those counters rather than only running scripts;
3. fault tests assert every durable phase required by the parent plan;
4. replay tests assert idempotent exact duplicates and duplicate mismatch
   rejection;
5. cache and durable tests assert visible-version safety;
6. no mutation-probe scratch file is present in the repository.

Suggested test names:

1. `commit_runtime_closeout_generated_counters_cover_required_categories`
2. `commit_runtime_closeout_fault_windows_cover_required_phases`
3. `commit_runtime_closeout_replay_contracts_are_exercised`

Review rule: do not add tests that pass because planning documents or links
exist. That rule is enforced by code review, not by adding another doc-shape
test.

### 2. Source Boundary

`commit_runtime_source_guard.rs` must prove:

1. production `src/commit/` exposes no public transaction/session API;
2. production `src/commit/` does not export public commit runtime types at the
   crate root;
3. production `src/commit/` does not import engine crates;
4. production `src/commit/` does not import product DTOs or product vocabulary;
5. production `src/commit/` does not import `crate::table` internals directly;
6. production `src/commit/` does not import backend/layout/filesystem APIs;
7. production `src/commit/` does not use `std::env`, process-global mutable
   state, or global lazy caches;
8. `src/branch/`, `src/format/`, and `src/service/` do not import
   `crate::commit`;
9. testkit/fuzz imports of commit internals are isolated to test targets or the
   `testkit` feature.

### 3. Fuzz Target Inventory

`commit_runtime_fuzz_inventory.rs` and/or `commit_runtime_closeout.rs` must
prove:

1. `commit_runtime_batch` is registered and calls
   `check_commit_runtime_batch_contract`;
2. `commit_runtime_conflict` is registered and calls
   `check_commit_runtime_conflict_contract`;
3. `commit_runtime_durable` is registered and calls
   `check_commit_runtime_durable_contract`;
4. `commit_runtime_timeline` is registered and calls
   `check_commit_runtime_timeline_contract`;
5. no commit-runtime fuzz target only calls
   `check_commit_runtime_scaffold_contract`;
6. every target has a checked-in corpus directory;
7. every target has at least two non-empty seeds;
8. seed corpora cover at least one success route and one rejection or fault
   route per target, either by seed naming convention plus contract execution
   or by explicit corpus metadata in the testkit.

### 4. Generated Property Assurance

`commit_runtime_properties.rs` must assert:

1. at least one generated mutating commit is reached;
2. cache success and cache failure routes are reached;
3. durable success and durable failure routes are reached;
4. replay success and replay rejection routes are reached;
5. read-only diagnostics are reached and do not allocate;
6. conflict rejection routes are reached;
7. timeline lookups are reached, including duplicate timestamp version
   tiebreaks;
8. guard/quiesce rejection routes are reached;
9. visible-read parity checks are reached;
10. model-production parity checks are reached after each generated operation.

### 5. Fault Window Assurance

Fault tests must cover:

1. validation failure before allocation;
2. branch lifecycle failure before allocation;
3. conflict before allocation;
4. timestamp source failure before visibility;
5. WAL clean failure before durable acknowledgement;
6. WAL writer halted before durable acknowledgement;
7. WAL segment id overflow or segment-roll failure before durable
   acknowledgement;
8. WAL uncertain failure;
9. WAL success then L6 apply failure;
10. WAL success then visible publish failure;
11. cache L6 apply failure;
12. cache visible publish failure;
13. replay apply failure;
14. replay visible publish failure;
15. unresolved durable gate blocks later mutating commits;
16. branch guard releases after every failure.

Assertions:

1. allocated-version gaps are documented by phase;
2. visible version advances only after full apply and publication;
3. unresolved durable state is recorded only for post-WAL durable failures;
4. errors are typed and preserve source chains when a source exists;
5. user value bytes do not appear in error display/debug text.

### 6. Sensitivity Probe Ledger

The L7N porting-log section must record these probes:

| Probe | Required evidence |
|---|---|
| S1 | Read-only allocation mutation fails a direct read-only test and generated contract. |
| S2 | Mixed-version batch stamping mutation fails batch stamping invariants. |
| S3 | Mixed-timestamp batch stamping mutation fails timestamp and WAL parity tests. |
| S4 | Durable apply-before-WAL mutation fails ordering tests. |
| S5 | WAL append failure visible-success mutation fails durable fault tests. |
| S6 | Durable-but-not-visible collapse mutation fails phase classification tests. |
| S7 | Conflict-after-allocation mutation fails no-allocation conflict tests. |
| S8 | Blind-write rejection mutation fails preserved conflict-model tests. |
| S9 | Branch-mismatched batch mutation fails batch validation tests. |
| S10 | Missing timeline row mutation fails timeline tests. |
| S11 | Visible-before-full-apply mutation fails atomic visibility tests. |
| S12 | Ignored branch deleting marker mutation fails branch guard tests. |
| S13 | Quiesce/commit overlap mutation fails interleaving tests. |
| S14 | Replay duplicate mismatch success mutation fails replay mismatch tests. |
| S15 | Direct backend/layout import mutation fails source guard. |
| S16 | Public begin/commit/rollback API mutation fails source guard or closeout inventory. |

For each probe, record:

1. target file and function;
2. exact mutation shape;
3. implemented test, generated counter, or structural guard that would catch
   the mutation;
4. verification command that executes that evidence;
5. result;
6. whether a live mutation was run. If it was run, also record the failing test
   and that the edit was reverted. If it was not run, mark the probe as
   `Covered-by-test`, not `Mutation-run`.

### 7. Deferred Behavior Verification

The closeout review must confirm that these are not treated as L7 defects:

1. public user transaction sessions;
2. durable transaction ids and transaction-id allocator catch-up;
3. serializable isolation claims;
4. public branch lifecycle and branch-history commands;
5. cross-branch atomic commits;
6. process-open recovery orchestration;
7. checkpoint, compaction, WAL-retention, and snapshot scheduling;
8. backend repair/quarantine orchestration;
9. object-store multi-writer fencing beyond current L4 capabilities;
10. engine observer side effects;
11. query/index/search side effects;
12. public storage API response DTOs.

Any other uncovered behavior must become either a concrete L7 bug or a new
explicit deferral with owner and rationale.

## Command Matrix

Run in this order before closing L7:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib commit
cargo test -p strata-storage-next --locked --lib commit_runtime --features testkit
cargo test -p strata-storage-next --locked --test commit_runtime_source_guard
cargo test -p strata-storage-next --locked --test commit_runtime_fuzz_inventory --features testkit
cargo test -p strata-storage-next --locked --test commit_runtime_closeout --features testkit
cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit
cargo test -p strata-storage-next --locked --test commit_runtime_faults --features "testkit fault-injection"
cargo test -p strata-storage-next --all-features --locked --test commit_runtime_properties --test commit_runtime_faults --test commit_runtime_fuzz_inventory --test commit_runtime_closeout
cargo test -p strata-storage-next --no-default-features --locked commit
cargo check -p strata-storage-next --no-default-features --locked --tests
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo check --manifest-path crates/storage-next/fuzz/Cargo.toml --locked --bins
cargo hack check -p strata-storage-next --feature-powerset --depth 2
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

Optional when nightly/libfuzzer is available:

```bash
cargo +nightly fuzz run commit_runtime_batch -- -max_total_time=60
cargo +nightly fuzz run commit_runtime_conflict -- -max_total_time=60
cargo +nightly fuzz run commit_runtime_durable -- -max_total_time=60
cargo +nightly fuzz run commit_runtime_timeline -- -max_total_time=60
```

If `cargo hack` is unavailable, install it or record the missing tool as a
closeout blocker. If nightly fuzzing is unavailable, record that explicitly in
the L7N command evidence. The non-optional cargo test, check, clippy, format,
feature-matrix, and diff commands must still pass.

## Exit Gate

L7N can close when:

1. closeout inventory tests pass;
2. source guards pass;
3. fuzz inventory passes;
4. generated property tests pass with required counters;
5. fault-window tests pass;
6. sensitivity probes are recorded with mutation target, verification command,
   and live-mutation status;
7. the command matrix passes or optional tool absence is documented;
8. no mutation artifacts or unreviewed generated files remain;
9. L7 deferred behavior is explicit and owned by later layers;
10. L8 can begin recovery orchestration using L7 replay and unresolved durable
    hooks.
