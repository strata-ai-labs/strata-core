# L7N Implementation Plan: L7 Conformance Closeout

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7n-l7-conformance-closeout-test-plan.md`

## Objective

L7N closes the M4-L7 commit-runtime milestone. It does not add a new commit
feature. It proves that the behavior built in L7A through L7M is ready for L8
recovery orchestration by consolidating:

1. implementation inventory;
2. source-boundary guards;
3. generated/property/fuzz/fault assurance;
4. sensitivity-probe evidence;
5. command-matrix evidence;
6. deferred behavior mapping.

L7N may fix bugs discovered during closeout. It must not introduce public
transaction/session APIs, product DTOs, backend/filesystem coupling, or L8
process-open recovery behavior.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/commit-timeline-substrate.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
5. All L7A through L7M implementation and test plans under
   `docs/architecture/implementation-plans/M4/L7/`
6. `docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md`
7. `crates/storage-next/src/commit/`
8. `crates/storage-next/src/testkit/commit_runtime*.rs`
9. `crates/storage-next/tests/commit_runtime_*.rs`
10. `crates/storage-next/fuzz/Cargo.toml`
11. `crates/storage-next/fuzz/fuzz_targets/commit_runtime_*.rs`
12. `crates/storage-next/fuzz/corpus/commit_runtime_*`

## Current State

L7A through L7M have built:

1. commit batch validation and row stamping;
2. version and timestamp allocation;
3. commit outcomes and visible-version tracking;
4. branch registry, branch guards, and quiesce hooks;
5. read-set and CAS conflict validation;
6. commit timeline rows and lookup helpers;
7. cache/no-WAL commit path;
8. durable WAL-before-visible commit path;
9. durable-but-not-visible classification and unresolved gate;
10. replay and allocator catch-up hooks;
11. concurrency/quiesce hardening;
12. generated model, fuzz targets, corpora, and fault scripts.

L7N should treat that surface as the subject under test. If a closeout check
finds a behavioral bug, fix the bug in the owning module and record it in the
porting log.

## Scope

L7N implements:

1. `crates/storage-next/tests/commit_runtime_closeout.rs`;
2. any missing implementation-focused checks in
   `crates/storage-next/tests/commit_runtime_source_guard.rs`;
3. any missing implementation-focused checks in
   `crates/storage-next/tests/commit_runtime_fuzz_inventory.rs`;
4. a final L7N section in `m4-l7-porting-log.md`;
5. a sensitivity-probe ledger with mutation target, expected failing test, and
   result;
6. command-matrix evidence for default, no-default, all-features, wasm, fuzz
   build, and clippy paths;
7. explicit closeout deferrals for work owned by L8, L9, engine-next, or
   post-V1.

L7N does not implement:

1. public transaction sessions;
2. public storage API wrappers;
3. L8 process-open replay orchestration;
4. real backend crash/reopen recovery discovery;
5. checkpoint, compaction, WAL-retention, or snapshot scheduling;
6. query/index/search side effects;
7. documentation-only tests that merely assert plan files or links exist.

## Closeout Principles

1. Automated closeout tests must exercise implementation artifacts or source
   boundaries, not planning-document structure.
2. Source guards should fail on real boundary regressions: public API leakage,
   forbidden imports, transaction-session vocabulary, and product vocabulary.
3. Fuzz inventory should prove each target is registered, seeded, and routed to
   a distinct contract function.
4. Generated and fault tests should keep proving runtime behavior; closeout
   should verify that those tests cover required categories.
5. Sensitivity probes are recorded as evidence, not simulated by committed
   mutation code.
6. If a requirement is intentionally deferred, the deferral must name the owner
   layer and rationale.

## File Layout

Preferred additions:

```text
crates/storage-next/tests/commit_runtime_closeout.rs
```

Possible updates:

```text
crates/storage-next/tests/commit_runtime_source_guard.rs
crates/storage-next/tests/commit_runtime_fuzz_inventory.rs
docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md
```

Do not add closeout assertions that inspect whether this file or the parent
plan exists. The docs are reviewed by humans; automated tests stay focused on
runtime assurance and source boundaries.

## Implementation Steps

### L7N-A: Inventory The Landed Runtime

Read the L7 runtime code and tests and produce an implementation inventory in
the porting log:

1. production commit modules;
2. testkit model/script/runner modules;
3. direct tests;
4. generated property tests;
5. fault tests;
6. source guards;
7. fuzz targets and corpora;
8. no-default and wasm compile surfaces.

Confirm that all production commit runtime items remain crate-private unless a
specific later public boundary requires otherwise.

### L7N-B: Add Closeout Inventory Tests

Create `commit_runtime_closeout.rs` with implementation-focused checks.

Required checks:

1. generated harness exposes counters for cache, durable, replay, conflict,
   timeline, guard/quiesce, fault, and parity categories;
2. property tests assert those counters rather than only running scripts;
3. source guard covers public transaction API, public commit runtime leakage,
   engine/product imports, table-internal imports, backend/layout/filesystem
   imports, and upward imports into lower layers;
4. fuzz targets are registered in `fuzz/Cargo.toml`;
5. fuzz target files call distinct contract functions;
6. no fuzz target routes only to the broad scaffold contract;
7. checked-in corpora exist for each target and contain non-empty seeds;
8. each corpus has at least one seed intended for success and one intended for
   rejection or fault behavior;
9. testkit-only helper imports stay behind test targets or the `testkit`
   feature.

Closeout inventory may inspect source files for structural rules, because the
source files are implementation artifacts. It must not inspect planning docs to
prove correctness.

### L7N-C: Consolidate Source Guards

Review `commit_runtime_source_guard.rs` against the parent source-guard policy.
Extend it only for real boundary gaps.

It must reject:

1. public `begin`, `commit`, `rollback`, transaction, or session APIs from the
   storage-next commit runtime;
2. production imports from engine crates;
3. production imports from product DTOs or product vocabulary modules;
4. direct production imports from `crate::table` internals;
5. direct production imports from `crate::backend`, `crate::layout`, object-name
   builders, filesystem APIs, environment variables, mmap, or process-global
   mutable state;
6. lower layers importing upward from `crate::commit`.

### L7N-D: Record Sensitivity Probes

Add a final sensitivity ledger in `m4-l7-porting-log.md`.

Each row must include:

1. probe id;
2. mutation target file and function;
3. mutation description;
4. implemented test, generated counter, or structural guard that would catch the
   mutation;
5. verification command that executes that evidence;
6. live-mutation status and final disposition.

Minimum probes:

| Probe | Mutation | Expected failure |
|---|---|---|
| S1 | Allocate a version for a read-only batch. | Read-only direct test and generated contract fail. |
| S2 | Stamp two rows in one batch with different versions. | Batch stamping invariant fails. |
| S3 | Stamp two rows in one batch with different timestamps. | Timestamp invariant and WAL record parity fail. |
| S4 | Apply to L6 before WAL append in durable mode. | WAL-before-visible ordering test fails. |
| S5 | Treat WAL append failure as visible success. | Durable fault-window test fails. |
| S6 | Collapse durable-but-not-visible into clean failure. | Phase classification test fails. |
| S7 | Validate conflicts after version allocation. | Conflict/no-allocation test fails. |
| S8 | Reject blind writes as conflicts. | Preserved conflict-model test fails. |
| S9 | Allow branch-mismatched row in batch. | Batch validation tests fail. |
| S10 | Omit timeline row generation. | Timeline direct/property tests fail. |
| S11 | Publish visible version before full L6 apply. | Atomic visibility property fails. |
| S12 | Ignore branch deleting marker. | Branch guard test fails. |
| S13 | Allow quiesce and mutating commit concurrently. | Quiesce interleaving property fails. |
| S14 | Replay duplicate mismatch as success. | Replay mismatch test fails. |
| S15 | Import backend or layout directly from `commit/`. | Source guard fails. |
| S16 | Expose public begin/commit/rollback API. | Source guard or closeout inventory fails. |

Mutation edits must not remain in the committed tree. If a probe is verified by
the implementation evidence rather than a live mutation run, mark it
`Covered-by-test` instead of claiming that a mutation was run.

### L7N-E: Execute The Command Matrix

Run and record the full closeout matrix:

1. default feature tests;
2. no-default feature checks;
3. all-features tests;
4. wasm no-default compile check;
5. fuzz binary compile check;
6. clippy with all targets and all features;
7. formatting and whitespace checks;
8. optional nightly fuzz smoke runs when available.

Command evidence belongs in the porting log, not in automated tests that assert
the log exists.

### L7N-F: Close The Deferred Map

Verify that every unimplemented behavior is either:

1. not part of L7;
2. explicitly owned by L8, L9, engine-next, or post-V1;
3. converted into a concrete L7 bug and fixed before closeout.

Expected deferrals:

1. public user transaction sessions;
2. durable transaction ids and transaction-id allocator catch-up;
3. serializable isolation claims;
4. public branch merge/cherry-pick/revert/restore operations;
5. cross-branch atomic commits;
6. branch-id reuse public lifecycle policy;
7. checkpoint scheduling;
8. compaction scheduling;
9. WAL retention scheduling;
10. snapshot creation scheduling;
11. process-open recovery orchestration;
12. backend repair/quarantine orchestration;
13. object-store multi-writer fencing beyond current L4 capabilities;
14. engine observer side effects;
15. query/index/search side effects;
16. public storage API response DTOs.

### L7N-G: Final Review

Before closing L7:

1. review every changed source file;
2. check for overly large files introduced during L7;
3. verify no generated or mutation-probe artifacts remain untracked;
4. ensure the only remaining worktree changes are intentional;
5. record the final commit hash and command evidence after the implementation
   is committed.

## Exit Gate

L7N is complete when:

1. `commit_runtime_closeout.rs` passes and checks implementation artifacts, not
   planning docs;
2. source guards pass and cover the full L7 boundary;
3. fuzz inventory passes and proves distinct target routing plus seed corpora;
4. generated property and fault tests pass with required category counters;
5. sensitivity probes are recorded with mutation targets and observed failing
   tests;
6. the command matrix passes or any unavailable optional command is documented;
7. deferred behavior is mapped to a later owner;
8. no new public transaction/session surface exists;
9. L8 has enough replay, unresolved durable, and allocator catch-up hooks to
   begin recovery orchestration.
