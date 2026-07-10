# L8G Test Plan: Commit Bootstrap And Recovery Health

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that L8G consumes an L8F recovery package, replays durable WAL records
through L7 replay, restores visible/allocator facts, finalizes recovery health,
and opens a durable-local runtime only after L6/L7 state is coherent.

Tests should fail if L8G:

1. opens before WAL replay succeeds;
2. reimplements L7 replay logic instead of calling `CommitReplayRuntime`;
3. runs normal conflict validation during recovery replay;
4. allocates new commit versions or timestamps for recovered WAL records;
5. ignores checkpoint-only visible-version restoration;
6. allows the next generated commit version to collide with recovered rows;
7. publishes visible facts before replayed rows are installed in L6;
8. accepts timeline-only, missing-timeline, or mismatched-timeline WAL records;
9. clears an unresolved durable gate without an exact replay match;
10. drops lower-layer commit/replay source errors;
11. reports `Healthy` after replay/bootstrap failure;
12. exposes ordinary reads or commits while lifecycle state is still
    `Recovering`;
13. starts maintenance or product reconstruction;
14. hardcodes object-layout paths or imports raw filesystem APIs.

Do not add tests whose only assertion is that plan documents exist or link to
other plan documents.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/bootstrap.rs` for direct L8G unit
   tests.
2. `crates/storage-next/src/lifecycle/tests/recovery.rs` only for shared L8F
   package helpers when the helper is already recovery-owned.
3. `crates/storage-next/src/lifecycle/tests/durable.rs` for post-open durable
   runtime smoke coverage if the open runtime surface lives in
   `lifecycle/durable/bootstrap.rs`.
4. `crates/storage-next/src/testkit/lifecycle/bootstrap.rs` for generated
   bootstrap scripts, counters, and model checks.
5. `crates/storage-next/tests/lifecycle_recovery.rs` for memory and localfs
   integration tests.
6. `crates/storage-next/tests/lifecycle_properties.rs` for generated lifecycle
   recovery properties behind `testkit`.
7. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
8. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for the
   L8G verification and sensitivity-probe record after implementation.

## Test Data Principles

1. Build WAL records through L7/L4 helpers whenever possible.
2. Corrupt WAL payload bytes only for explicit replay-validation tests.
3. Use storage-owned rows and keys, not product primitive DTOs.
4. Keep canonical smoke fixtures separate from generated-input coverage.
5. Generated tests must count input-derived operations separately from fixed
   setup operations.
6. Localfs tests may be feature/platform gated, but memory tests must exercise
   the L8G contract by default.
7. Avoid reserved layout string literals in test fixture names.
8. Use existing L7 replay assertions where possible instead of duplicating
   replay internals in lifecycle tests.

## Direct Unit Tests

### 1. Admission And Lifecycle State

Required tests:

1. `bootstrap_requires_recovering_durable_shell`
2. `bootstrap_rejects_cache_shell`
3. `bootstrap_rejects_object_candidate_shell`
4. `bootstrap_rejects_failed_recovery_health`
5. `ordinary_reads_remain_rejected_before_bootstrap_success`
6. `commits_remain_rejected_before_bootstrap_success`
7. `ordinary_maintenance_remains_rejected_before_bootstrap_success`
8. `bootstrap_transitions_recovering_to_open_after_success`
9. `bootstrap_failure_leaves_shell_not_open`
10. `bootstrap_is_not_rerunnable_after_open_without_explicit_reopen`

Assertions:

1. L8G checks lifecycle state before replay;
2. invalid modes fail before mutating branch, allocator, visible tracker, or
   durable gate;
3. success transitions through `RecoveryAccepted`;
4. failure preserves enough recovery facts for diagnostics;
5. no test observes partial state through ordinary read/commit admission.

### 2. Recovery Package Validation

Required tests:

1. healthy empty L8F outcome is accepted;
2. degraded L8F outcome is accepted only if the open plan allowed degraded
   recovery;
3. failed L8F outcome is rejected;
4. WAL records must target the shell's branch in V1;
5. mixed-branch WAL package rejects before opening;
6. records out of commit-version order fail closed;
7. duplicate WAL commit versions fail closed unless L7 reports exact
   idempotent replay for the same durable record;
8. checkpoint watermark above installed state rejects;
9. checkpoint watermark below replayed WAL tail is accepted;
10. replay-start metadata from L8F is treated as advisory for validation, not
    as permission to skip packaged records.

### 3. Empty Recovery

Required tests:

1. empty durable recovery opens with visible version zero;
2. open outcome reports durable mode;
3. open outcome reports healthy recovery;
4. maintenance readiness is false for V1;
5. allocator next generated version is one;
6. durable gate is empty;
7. ordinary read admission succeeds after open;
8. a normal durable commit can execute after open.

### 4. Checkpoint-Only Bootstrap

Required tests:

1. checkpoint-only package publishes visible version to the trusted watermark;
2. checkpoint-only package with watermark zero opens as empty;
3. checkpoint-only recovered rows are readable after open;
4. checkpoint-only recovered tombstones remain tombstones after open;
5. checkpoint-only branch facts do not become visible until L8G catch-up;
6. allocator catches up above the checkpoint watermark;
7. next generated commit version is greater than the checkpoint watermark;
8. timestamp guard is not fabricated when checkpoint metadata has no trusted
   timestamp watermark;
9. checkpoint visible catch-up failure fails bootstrap before `Open`;
10. checkpoint watermark above branch max version fails closed.

### 5. WAL Replay Bootstrap

Required tests:

1. WAL-only package replays records through `CommitReplayRuntime`;
2. checkpoint-plus-WAL package replays only packaged WAL records;
3. replay preserves WAL commit version;
4. replay preserves WAL commit timestamp;
5. replay preserves branch id;
6. replay preserves storage rows;
7. replay installs user mutation rows into L6;
8. replay installs matching timeline rows through L7 replay;
9. final visible version equals latest replayed commit version when newer than
   checkpoint watermark;
10. exact duplicate replay is idempotent;
11. installed mismatch fails closed;
12. partial installed state fails closed;
13. replay source errors preserve source chains;
14. replay does not run normal conflict validation;
15. replay does not allocate new versions or timestamps.

### 6. Durability Class Mapping

Required tests:

1. `DurableLocalStandard` maps to `CommitDurabilityClass::Standard`;
2. `DurableLocalAlways` maps to `CommitDurabilityClass::Always`;
3. standard replay rejects records that are not standard-compatible;
4. always replay rejects records that are not always-compatible;
5. cache mode never reaches replay;
6. object-candidate mode never reaches replay;
7. invalid mode failure leaves visible tracker unchanged.

### 7. Timeline Validation

Required tests:

1. WAL record with matching timeline rows succeeds;
2. WAL record missing timeline rows rejects;
3. WAL record with only timeline rows and no user rows rejects;
4. WAL record with mismatched timeline branch rejects;
5. WAL record with mismatched timeline version rejects;
6. WAL record with mismatched timeline timestamp rejects;
7. duplicate timeline rows reject;
8. timeline mismatch maps to timeline-specific recovery health or preserves
   the L7 replay error source;
9. as-of lookup after recovery finds the replayed timeline entry;
10. duplicate timestamps resolve by greatest retained version at or before the
    requested timestamp.

### 8. Allocator And Timestamp Catch-Up

Required tests:

1. replayed WAL record catches up version allocator;
2. multiple replayed WAL records catch up to the maximum recovered version;
3. exact duplicate replay still catches allocator above the duplicate version;
4. checkpoint-only recovery catches allocator above checkpoint watermark;
5. checkpoint-plus-WAL recovery catches allocator above the WAL tail;
6. allocator catch-up failure fails bootstrap before `Open`;
7. replayed WAL timestamp catches up timestamp guard;
8. timestamp guard rejects later generated timestamp regression;
9. no transaction-id allocator exists or is initialized by L8G;
10. allocator state is unchanged after package-validation failure.

### 9. Visible-Version Restoration

Required tests:

1. visible version starts below recovered rows during `Recovering`;
2. visible version advances only after L7 replay applies rows;
3. final visible version is max of checkpoint watermark and replayed WAL tail;
4. visible version does not advance on replay mismatch;
5. visible version does not advance on partial installed state;
6. visible version does not advance on timeline mismatch;
7. visible publication failure after apply returns typed failure and preserves
   unresolved durable facts;
8. no branch can be ahead of global visible version after successful bootstrap;
9. ordinary reads after open are capped by the restored visible version;
10. normal commit after open advances visible monotonically.

### 10. Unresolved Durable Gate Reconciliation

Required tests:

1. empty gate remains empty after successful replay;
2. matching unresolved gate is cleared by exact replay;
3. different unresolved gate blocks replay;
4. unresolved durable-but-not-applied replay installs rows then clears gate;
5. unresolved durable-but-not-visible replay publishes visibility then clears
   gate;
6. replay apply failure records or preserves unresolved durable state;
7. visible publication failure records or preserves unresolved durable state;
8. gate mismatch failure keeps the old gate intact;
9. normal commits after successful bootstrap are admitted only when gate is
   empty;
10. normal commits after bootstrap failure remain rejected.

### 11. Recovery Health And Open Outcome

Required tests:

1. healthy L8F plus successful bootstrap reports healthy outcome;
2. degraded L8F plus successful bootstrap preserves degraded outcome;
3. bootstrap failure never reports healthy;
4. replay mismatch reports failed recovery health or typed lifecycle error;
5. timeline mismatch reports failed recovery health or typed lifecycle error;
6. visible catch-up failure reports failed recovery health or typed lifecycle
   error;
7. open outcome recovered visible version equals final visible tracker;
8. open outcome mode equals the durable shell mode;
9. open outcome disposition from L8E is preserved;
10. maintenance readiness is false until later maintenance slices opt in.

### 12. Post-Open Durable Runtime Smoke

Required tests:

1. recovered latest read returns replayed value;
2. recovered as-of read returns replayed value at timestamp;
3. recovered tombstone hides latest value;
4. normal durable put after recovery succeeds;
5. normal durable delete after recovery succeeds;
6. normal durable commit uses next version above recovered watermark;
7. same-branch guard still serializes commits after recovery;
8. unresolved gate still blocks commits after recovery when intentionally left;
9. close remains minimal/idempotent if exposed by this slice;
10. close does not start maintenance.

## Integration Tests

### Memory Backend

Required tests in `tests/lifecycle_recovery.rs`:

1. create durable standard runtime, commit rows, reopen, replay WAL, read rows;
2. create durable always runtime, commit rows, reopen, replay WAL, read rows;
3. checkpoint-only recovery opens and reads checkpoint rows;
4. checkpoint plus WAL tail opens at WAL tail;
5. duplicate reopen is idempotent;
6. corrupt WAL payload fails recovery before open;
7. missing timeline rows fail recovery before open;
8. visible publication fault after replay is classified;
9. branch apply fault after replay is classified;
10. normal durable commit after recovery persists across another reopen.
11. `lifecycle_bootstrap_contract_exercises_commit_bootstrap_paths` runs the
    testkit bootstrap contract against the in-memory backend and asserts
    counters for empty bootstrap, checkpoint catch-up, WAL replay, degraded
    health preservation, replay rejection, and input-derived variants.

### Local Filesystem Backend

When `localfs` is enabled and the platform supports the writer guard:

1. same checkpoint-only recovery test as memory;
2. same checkpoint-plus-WAL recovery test as memory;
3. reopen after exact duplicate replay remains idempotent;
4. writer guard prevents a second recovery bootstrap from the same root;
5. localfs object layout source guard remains clean.

## Generated Properties

Add or extend lifecycle generated scripts with operations:

1. start empty durable recovery;
2. add checkpoint row group;
3. add WAL record after checkpoint;
4. add exact duplicate WAL record;
5. add WAL mismatch;
6. add partial installed row;
7. add missing timeline row;
8. add timeline mismatch;
9. add degraded L8F health;
10. add matching unresolved gate;
11. add mismatched unresolved gate;
12. inject branch apply failure;
13. inject visible publication failure;
14. bootstrap;
15. read after open;
16. commit after open.

Counters must distinguish:

1. input-derived empty recovery;
2. input-derived checkpoint-only recovery;
3. input-derived WAL replay;
4. input-derived exact duplicate replay;
5. input-derived mismatch failure;
6. input-derived timeline failure;
7. input-derived gate reconciliation;
8. input-derived allocator catch-up;
9. input-derived visible catch-up;
10. input-derived degraded-health preservation.

The property harness must run `check_lifecycle_bootstrap_contract` separately
from the L8F recovery contract. It must not satisfy bootstrap counters only by
calling `check_lifecycle_recovery_contract` or by prepending a fixed canonical
script without also recording input-derived bootstrap cases.

## Fault Injection

Fault tests must cover:

1. package validation failure before replay;
2. L7 replay request validation failure;
3. L6 branch apply failure during replay;
4. visible publication failure after apply;
5. durable gate record failure if the replay runtime records unresolved state;
6. allocator catch-up failure;
7. checkpoint visible catch-up failure;
8. open-outcome validation failure;
9. lifecycle transition failure;
10. post-open commit failure after recovered state is visible.

For each fault, assert:

1. whether rows were applied;
2. whether visible version moved;
3. whether allocator moved;
4. whether unresolved gate changed;
5. whether lifecycle state became `Open`;
6. whether source chain is preserved.

## Source Guards

`tests/lifecycle_source_guard.rs` should assert:

1. L8G may import `crate::commit::replay`;
2. L8G may import L7 allocator, visible, durable runtime, and durable gate
   types needed for bootstrap;
3. L8G does not import product, engine, IPC, follower, StrataHub, or remote
   sync modules;
4. L8G does not import raw `std::fs`, `std::path::Path`, `std::env`, mmap, or
   direct file APIs;
5. L8G does not import L4 snapshot/table/quarantine services for new recovery
   reads;
6. L8G does not call WAL read/repair APIs owned by L8F;
7. lower layers do not import `crate::lifecycle`;
8. no production L8G surface is `pub` unless L9 explicitly wraps it.

## Sensitivity Probes

Record these in the porting log after implementation:

1. skip L7 replay and append rows directly to L6;
2. replay with newly allocated commit versions;
3. publish visible version before branch apply;
4. ignore checkpoint-only visible catch-up;
5. ignore checkpoint-only allocator catch-up;
6. accept timeline-only WAL payload;
7. ignore timeline timestamp mismatch;
8. clear mismatched unresolved durable gate;
9. convert degraded L8F health to healthy;
10. allow normal commit while state is `Recovering`;
11. report `Open` after replay failure;
12. allow object-candidate mode to run L8G bootstrap.

Each porting-log row should name the mutated file/line, the mutation, and the
test that failed.

## Deferred

1. Multi-branch runtime maps and mixed-branch WAL package replay: L9 or later
   L8 extension.
2. Flushed table-state recovery beyond checkpoint row install: L8I/L8J.
3. Maintenance readiness beyond conservative `false`: L8H and later.
4. Background task start/drain around recovery: L8H/L8N.
5. Public storage open API and product primitive reconstruction: L9 and engine
   layer.
6. Crash-process harnesses that kill the process at every L8G phase: L8O unless
   pulled forward.

## Verification Commands

Run after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::bootstrap
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

If localfs is enabled and supported:

```bash
cargo test -p strata-storage-next --features localfs --locked --test lifecycle_recovery
```

## Closeout Checklist

L8G can close when:

1. every successful bootstrap reaches `Open` only after replay/bootstrap
   success;
2. empty, checkpoint-only, WAL-only, and checkpoint-plus-WAL paths pass;
3. L7 replay is the only WAL replay path;
4. checkpoint-only visible and allocator catch-up are pinned;
5. exact duplicate replay is idempotent;
6. mismatch, partial, missing-timeline, and timeline-mismatch cases fail
   closed;
7. matching and mismatched durable-gate reconciliation are pinned;
8. recovery health and `StorageOpenOutcome` preserve raw storage facts,
   including backend capabilities, database id, codec id, checkpoint/WAL/table/
   quarantine recovery facts, bootstrap facts, and raw stats;
9. post-open read and one durable commit work through L6/L7 surfaces;
10. source guards block product/raw IO drift;
11. generated properties exercise input-derived bootstrap categories;
12. porting log records verification and sensitivity probes.
