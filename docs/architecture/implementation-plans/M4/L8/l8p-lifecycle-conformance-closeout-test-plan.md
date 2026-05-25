# L8P Test Plan: Lifecycle Conformance Closeout

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8p-lifecycle-conformance-closeout-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that M4-L8 is closeable as a storage-internal lifecycle runtime.

The closeout suite should verify implementation behavior, source boundaries,
generated/fault/crash/fuzz assurance, command evidence, and deferral mapping.
It must not add tests whose only assertion is that planning documents exist,
link to each other, or contain a phrase.

## Test Locations

Use:

1. `crates/storage-next/tests/lifecycle_closeout.rs` for closeout inventory;
2. `crates/storage-next/tests/lifecycle_source_guard.rs` for source and
   boundary checks;
3. `crates/storage-next/tests/lifecycle_fuzz_inventory.rs` for fuzz target and
   corpus checks;
4. `crates/storage-next/tests/lifecycle_properties.rs` for generated model and
   property coverage;
5. `crates/storage-next/tests/lifecycle_faults.rs` for direct fault-window
   coverage;
6. `crates/storage-next/tests/crash_recovery.rs` for localfs crash/reopen
   coverage;
7. `crates/storage-next/tests/lifecycle_maintenance.rs` for non-crash
   integration coverage;
8. `crates/storage-next/tests/lifecycle_recovery.rs` for recovery/bootstrap
   integration coverage;
9. `crates/storage-next/src/testkit/lifecycle/` for model, script, fault, and
   crash contract counters;
10. `crates/storage-next/fuzz/fuzz_targets/lifecycle_*.rs` for fuzz target
    routing;
11. `crates/storage-next/fuzz/corpus/lifecycle_*` for seed corpora.

## Fixture Rules

1. Keep closeout tests deterministic and repository-local.
2. Do not spawn threads, sleep, or depend on wall-clock waits.
3. Do not mutate source files from tests.
4. Inspect source files only for implementation boundary rules.
5. Keep source scanning allowlists explicit and narrow.
6. Keep command evidence manual in the porting log.
7. Keep mutation probes out of the committed tree.
8. Do not put milestone labels in Rust test names, comments, fixture bytes, or
   panic messages.

## Direct Test Matrix

### 1. Closeout Inventory

Add `lifecycle_closeout.rs`.

Required cases:

1. generated script tests cover input-derived open/recovery, maintenance,
   reclaim, validation, visibility, deletion, watermark, close, cache-mode, and
   degraded-health routes;
2. property tests assert input-derived counters rather than only running
   scripts;
3. maintenance integration tests assert generated, fault, crash, retention,
   quarantine, rewrite, close, and health counters;
4. fault tests assert every phase family required by L8O;
5. crash tests assert every localfs crash/reopen family required by L8O;
6. recovery tests assert recovery and bootstrap generated coverage;
7. fuzz inventory tests assert exact target names, contract functions, and seed
   names;
8. source guards assert assurance placement, production/testkit separation,
   crash gating, no sleeps/threads, and implementation-label avoidance;
9. no mutation-probe scratch files are present.

Suggested test names:

1. `lifecycle_closeout_generated_counters_cover_required_categories`
2. `lifecycle_closeout_fault_windows_cover_required_phases`
3. `lifecycle_closeout_crash_windows_cover_required_phases`
4. `lifecycle_closeout_fuzz_targets_and_corpora_are_distinct`
5. `lifecycle_closeout_source_guards_cover_required_boundaries`
6. `lifecycle_closeout_has_no_mutation_probe_artifacts`

These tests may inspect source files because the source files are
implementation artifacts. They must not inspect planning docs to prove
correctness.

### 2. Source Boundary

`lifecycle_source_guard.rs` must prove:

1. production lifecycle source does not import engine, product, StrataHub, or
   public API modules;
2. production lifecycle source does not import testkit or fuzz helpers;
3. production lifecycle source does not use raw filesystem/env/process-global IO
   or grouped path APIs;
4. lower storage layers do not import `crate::lifecycle`;
5. lifecycle remains crate-private until L9;
6. cache runtime does not import durable services;
7. durable assembly, bootstrap, durable maintenance, durable close, recovery,
   checkpoint, flush, retention, quarantine, and table rewrite boundaries remain
   separated;
8. fuzz targets call distinct lifecycle fuzz contracts;
9. checked-in lifecycle fuzz corpora are non-empty;
10. crash tests are localfs/testkit/wasm gated;
11. assurance tests avoid sleeps and thread spawns.

### 3. Fuzz Target Inventory

`lifecycle_fuzz_inventory.rs` and/or `lifecycle_closeout.rs` must prove:

1. `lifecycle_recovery` is registered and calls
   `check_lifecycle_recovery_fuzz_contract`;
2. `lifecycle_maintenance` is registered and calls
   `check_lifecycle_maintenance_fuzz_contract`;
3. `lifecycle_retention` is registered and calls
   `check_lifecycle_retention_fuzz_contract`;
4. no lifecycle fuzz target calls only a generic scaffold contract;
5. `lifecycle_recovery` corpus contains `valid_seed`, `corrupt_seed`, and
   `mixed_seed`;
6. `lifecycle_maintenance` corpus contains `valid_seed`, `fault_seed`, and
   `close_seed`;
7. `lifecycle_retention` corpus contains `valid_seed`, `blocked_seed`, and
   `purge_seed`;
8. each seed file is non-empty;
9. seed contracts execute the intended valid/corrupt/task/close/delete/defer
   routes under normal tests.

### 4. Generated Property Assurance

`lifecycle_properties.rs` must assert:

1. generated lifecycle script contract runs under bounded proptest cases;
2. recovery/open/close routes are input-derived;
3. maintenance routes are input-derived;
4. retention routes are input-derived;
5. quarantine routes are input-derived;
6. close routes are input-derived;
7. validation-only generated scripts produce typed validation rejections;
8. generated model visible/checkpoint/flush watermarks are monotonic;
9. deletion set is a subset of model proof;
10. cache-mode generated scripts never claim durable recovery;
11. lossy generated recovery records degraded health;
12. minimized failure/regression path is stable.

### 5. Fault Window Assurance

`lifecycle_faults.rs` must cover:

1. capability mismatch before durable side effects;
2. writer guard acquired then manifest create failure;
3. manifest create visible but publish uncertain;
4. snapshot published but manifest update failed;
5. manifest updated but WAL truncation failed;
6. partial WAL tail strict failure before repair;
7. partial WAL tail lossy repair and degraded health;
8. corrupt WAL typed recovery error;
9. replay failure transitions bootstrap to failed;
10. replay visible publication failure records durable-not-visible;
11. flush table published but branch install failed;
12. table rewrite branch swap failure preserves reads;
13. incomplete retention proof blocks delete before backend access;
14. quarantine inventory publish failure blocks purge;
15. purge delete success but inventory update failure preserves debt;
16. close quiesce timeout is retryable;
17. close WAL sync failure preserves source chain;
18. close manifest sync failure preserves final fact debt;
19. missing writer guard at release is typed.

Deferred from V1 (recorded for transparency):

- **Backend-reported writer-guard release failure.** The earlier wording
  of scenario #19 required asserting a typed close error when the
  *backend itself* fails the release of an existing guard (e.g., an
  object-store lease renounce that returns Unavailable). V1's
  writer-guard release is infallible by construction: `release_writer_guard`
  on the durable runtime is a take-and-drop of an in-memory handle,
  and `LocalFsBackend::acquire_writer_lock` returns a guard whose only
  Drop work is releasing the OS advisory lock — neither path can
  return a typed failure. Making release fallible would require
  extending `BackendWriterGuard` (and every backend that constructs
  one) with a fallible-release hook, which is appropriate post-V1 when
  the object-backend work introduces lease-handoff semantics. Scenario
  #19 is therefore narrowed to "missing writer guard at release is
  typed" — the only failure mode the V1 API surface can express — and
  the backend-reported variant is reserved for post-V1 object-backend
  work. The closeout integration check (`tests/lifecycle_closeout.rs`
  `lifecycle_closeout_integration_surfaces_cover_required_categories`)
  references the narrowed scenario only.

Assertions:

1. errors assert `code()`, not display strings;
2. lower-layer source chains are preserved where applicable;
3. lifecycle state after failure is asserted by direct/unit coverage or fault
   contract;
4. retryability is asserted;
5. health/debt facts name affected object families where known;
6. unsafe deletion does not occur.

### 6. Crash/Reopen Assurance

`crash_recovery.rs` must cover:

1. crash after WAL append before visibility replays record;
2. crash after WAL append with unresolved gate reconciles on reopen;
3. crash after snapshot publish before manifest update ignores orphan snapshot;
4. crash after manifest update before WAL truncation recovers checkpoint and
   tail;
5. crash after table publish before branch install reports orphan table;
6. crash after quarantine inventory publish before object move reports debt;
7. crash after object quarantine before purge preserves quarantine entry;
8. crash after close WAL sync before guard release reopens consistently;
9. ignored crash cases have nonignored phase equivalents;
10. harness respects case limit and keep-root environment.

Rules:

1. localfs durable tests are cfg-gated with `feature = "localfs"`,
   `feature = "testkit"`, and `not(target_arch = "wasm32")`;
2. slow process-level tests may be `#[ignore]`;
3. every ignored crash test has a nonignored unit/integration equivalent;
4. tests use deterministic temp roots;
5. tests do not sleep or spawn unbounded background work.

### 7. Integration Assurance

`lifecycle_maintenance.rs` and `lifecycle_recovery.rs` must prove:

1. generated default-mode script runs;
2. generated durable-mode script runs;
3. generated reclaim/close script runs;
4. fault integration covers all phase families;
5. crash integration reports case counts;
6. recovery contract exercises storage recovery paths;
7. bootstrap contract exercises commit bootstrap paths;
8. generated recovery routes are input-driven;
9. generated bootstrap catches allocator, timestamp, visible, timeline, and
   unresolved-gate facts.

### 8. Sensitivity Probe Ledger

The L8P porting-log section must record these probes:

| Probe | Required evidence |
|---|---|
| S1 | Cache durable-claim mutation fails cache outcome or generated cache-mode tests. |
| S2 | Capability-preflight bypass mutation fails capability-order tests. |
| S3 | Bootstrap failure state mutation fails recovery/bootstrap tests. |
| S4 | Visible-version over-advance mutation fails bootstrap/generated recovery tests. |
| S5 | Strict WAL-tail repair mutation fails recovery strict-tail tests. |
| S6 | Lossy missing snapshot healthy mutation fails recovery health tests. |
| S7 | Flush watermark/WAL mutation fails flush direct/source tests. |
| S8 | Opaque checkpoint section rejection mutation fails checkpoint recovery tests. |
| S9 | Over-aggressive WAL truncation mutation fails generated checkpoint/crash tests. |
| S10 | Naked materialization layer-index mutation fails materialization tests. |
| S11 | Reachable table deletion mutation fails retention tests. |
| S12 | Live snapshot pruning mutation fails snapshot pruning tests. |
| S13 | Stale purge proof mutation fails purge/quarantine tests. |
| S14 | Repair mutation/invention fails repair tests. |
| S15 | Close ordinary-work mutation fails close/generated tests. |
| S16 | Guard-before-sync mutation fails durable close tests. |
| S17 | Shared fuzz scaffold mutation fails fuzz inventory/source guard. |
| S18 | Crash cfg mutation fails crash source guard. |
| S19 | Testkit/fuzz production import mutation fails source guard. |
| S20 | Engine/product production import mutation fails source guard. |

For each probe, record:

1. target file and function;
2. mutation shape;
3. implemented test, generated counter, or structural guard that catches it;
4. verification command;
5. result;
6. live-mutation status.

If no live mutation was run, mark the probe `Covered-by-test`.

### 9. Deferral Verification

Closeout must confirm these are not treated as L8 defects:

1. public lifecycle/open/close API wrappers;
2. public maintenance commands;
3. product recovery wording;
4. primitive reconstruction;
5. engine observer callbacks;
6. background worker thread scheduling;
7. process-kill matrix across every phase in default CI;
8. distributed object-store lease races;
9. StrataHub sync/push/pull behavior;
10. query/index/search side effects;
11. final memory-budget tuning.

Any other uncovered behavior must become either a concrete L8 bug or a new
explicit deferral with owner and rationale.

## Command Matrix

Run in this order before closing L8:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test lifecycle_faults
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_fuzz_inventory
cargo test -p strata-storage-next --features localfs,testkit --locked --test crash_recovery
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_closeout
cargo test -p strata-storage-next --all-features --locked --test lifecycle_closeout --test lifecycle_source_guard --test lifecycle_fuzz_inventory
cargo test -p strata-storage-next --no-default-features --locked lifecycle
cargo check -p strata-storage-next --no-default-features --locked --tests
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo check --manifest-path crates/storage-next/fuzz/Cargo.toml --locked --bins
cargo hack check -p strata-storage-next --feature-powerset --depth 2
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

Optional when nightly/libfuzzer is available:

```bash
cargo +nightly fuzz run lifecycle_recovery -- -max_total_time=60
cargo +nightly fuzz run lifecycle_maintenance -- -max_total_time=60
cargo +nightly fuzz run lifecycle_retention -- -max_total_time=60
```

If `cargo hack` or nightly fuzzing is unavailable, record that in the porting
log. The non-optional cargo test, check, clippy, format, fuzz-build, and diff
commands must still pass unless an explicit environment blocker is recorded.

## Exit Gate

L8P can close when:

1. closeout inventory tests pass;
2. source guards pass;
3. fuzz inventory and fuzz binary build pass;
4. generated, integration, fault, crash, recovery, maintenance, and lifecycle
   unit suites pass;
5. clippy, fmt, and whitespace checks pass;
6. no-default, all-features, and wasm compile checks pass;
7. cargo-hack feature matrix passes or a missing-tool blocker is recorded;
8. optional nightly fuzz status is recorded;
9. sensitivity probes are recorded with concrete evidence;
10. final deferrals are explicit and owner-tagged;
11. porting log marks L8A through L8P accurately;
12. working tree contains no mutation-probe scratch files.
