# Engine Stabilization Plan

## Purpose

Engine consolidation is closed. Before starting the v1 architecture design
phase, the engine test baseline should be green again.

This plan fixes the 10 known `cargo test -p strata-engine` failures recorded at
`EG9E`. They are not graph or storage-boundary failures, but they cover
important engine reliability behavior:

- WAL background sync failure publication
- WAL writer halt and resume semantics
- shutdown timeout interleavings while the writer is halted
- runtime durability-mode rollback
- branch delete cleanup failure classification

The goal is a clean, trusted engine baseline before writing the target
architecture, engine-next, storage-next, testing strategy, and product
experience documents.

## Scope

This stabilization owns:

- restoring the failing engine tests without weakening their invariants
- repairing stale or broken engine test/fault-injection hook routing
- preserving the intended production behavior around WAL halts and branch
  cleanup classification
- updating closeout docs once the engine package is green

This stabilization does not own:

- reopening `EG` consolidation work
- engine-next or storage-next architecture
- changing WAL, manifest, checkpoint, snapshot, search, or vector physical
  formats
- replacing the shutdown or branch-delete designs wholesale
- silencing tests with `#[ignore]` or weakening assertions to match broken
  behavior

## Original Failure Ledger

The original failing tests were:

```text
database::branch_mutation::tests::test_rollback_delete_true_surfaces_storage_cleanup_failure
database::tests::shutdown::shutdown_timeout_halt_interleaving_preserves_invariant
database::tests::shutdown::shutdown_timeout_preserves_writer_halt_signal
database::tests::shutdown::test_background_sync_failure_halts_writer_and_rejects_manual_commit
database::tests::shutdown::test_begin_sync_failure_halts_writer_and_rejects_manual_commit
database::tests::shutdown::test_commit_sync_failure_halts_writer_and_rejects_manual_commit
database::tests::shutdown::test_resume_waits_for_inflight_halt_publication_before_restoring_accepting
database::tests::shutdown::test_resume_while_still_failing_increments_failed_sync_count
database::tests::shutdown::test_set_durability_mode_spawn_failure_rolls_back_state
primitives::branch::index::tests::test_complete_delete_post_commit_classifies_default_marker_clear_failure
```

Initial focused reruns showed two failure shapes:

- WAL/shutdown tests time out waiting for injected failures to halt the writer,
  or expect an injected flush-thread spawn failure that never fires.
- branch cleanup tests inject cleanup failures, but the cleanup path returns
  `Ok`/`Clean` instead of surfacing `PostCommitError`.

The highest-confidence initial hypothesis was path-key drift in test hooks:
tests inject failures using the temp path, while production open paths now
canonicalize and store `Database::data_dir()` as the hook lookup key. On macOS
and CI, raw temp paths and canonical paths can differ, causing all path-scoped
fault injections to miss.

## Stabilization Rules

1. Fix the fault-injection surface before changing production behavior.
   These failures are clustered around hooks not firing. Prove or disprove hook
   reachability first.

2. Do not replace precise assertions with sleeps or broad retries.
   If a timing-dependent test remains flaky after hook routing is fixed, add a
   deterministic test seam or explicit state wait.

3. Keep production semantics intact.
   WAL sync failures must latch `WalWriterHealth::Halted` and reject new and
   already-open commits until explicit resume. Post-commit branch cleanup errors
   must not be silently reported as clean.

4. Keep test-only machinery test-only.
   Any new helper belongs behind `#[cfg(test)]` or an existing test-support /
   fault-injection gate. Do not expose engine internals as product API.

5. Update docs only after the code is green.
   The `EG9` closeout ledger names the original 10 failures until this
   stabilization result is recorded there.

## STAB1A - Characterize Hook Reachability

**Goal:**

Prove whether the failing hooks miss because the injected path and database
runtime path differ.

**Work:**

- compare each test's injection path with `db.data_dir()`
- inspect all path-scoped hook families:
  - sync failure
  - begin-background-sync failure
  - commit-background-sync failure
  - flush-thread spawn failure
  - halt-publication pause
  - clear-branch-storage failure
  - clear-default-branch-marker failure
- add a tiny focused test or assertion that demonstrates raw path versus
  canonical path behavior if needed
- confirm whether any hook is intentionally injected before the database
  directory exists

**Acceptance:**

- root cause for missed hooks is named precisely
- every failing test is classified as path-key miss, timing gap, or real
  production bug
- no production behavior changes are made in this step

**Implementation, 2026-05-08:**

STAB1A confirmed path-key drift as the current root cause. The database open
paths canonicalize the on-disk directory and store that canonical path in
`Database::data_dir()`. The failing tests inject faults through raw temp paths
such as `/var/...`, while runtime hook lookups use canonical paths such as
`/private/var/...` on macOS. The same class of mismatch is reproducible without
platform-specific temp paths by injecting through a lexical alias like
`db/../db` and consuming through its canonical path.

The STAB1A characterization proved that every path-scoped engine hook family
used exact lexical `PathBuf` matching on insert, clear, and consume paths:

- sync failure
- begin-background-sync failure
- commit-background-sync failure
- flush-thread spawn failure
- halt-publication pause
- clear-branch-storage failure
- clear-default-branch-marker failure

It also proved both halt-publication directions: a canonical halt publisher did
not reach a pause installed under a raw alias, and a raw clear does not remove a
pause installed under the canonical key.

Failure classification:

| Test | Classification |
| ---- | -------------- |
| `test_set_durability_mode_spawn_failure_rolls_back_state` | path-key miss: test injects on raw `db_path`; flush-thread spawn consumes canonical `data_dir` |
| `test_background_sync_failure_halts_writer_and_rejects_manual_commit` | path-key miss: raw sync-failure injection is not visible to the background flush thread |
| `test_begin_sync_failure_halts_writer_and_rejects_manual_commit` | path-key miss: raw begin-sync injection is not visible to the background flush thread |
| `test_commit_sync_failure_halts_writer_and_rejects_manual_commit` | path-key miss: raw commit-sync injection is not visible to the background flush thread |
| `test_resume_while_still_failing_increments_failed_sync_count` | path-key miss: raw sync-failure injection is not visible to the resume/sync path |
| `test_resume_waits_for_inflight_halt_publication_before_restoring_accepting` | path-key miss: raw begin-sync and halt-pause injections are not visible to the halt publication path |
| `shutdown_timeout_preserves_writer_halt_signal` | path-key miss: raw sync-failure injection is not visible to the background flush thread |
| `shutdown_timeout_halt_interleaving_preserves_invariant` | path-key miss: raw sync-failure injection is not visible, so the stress loop never observes a halt |
| `test_rollback_delete_true_surfaces_storage_cleanup_failure` | path-key miss: raw tempdir cleanup injection is not visible to `clear_branch_storage_result()` |
| `test_complete_delete_post_commit_classifies_default_marker_clear_failure` | path-key miss: raw tempdir marker-clear injection is not visible to `clear_default_branch_marker_if()` |

No timing gap or production behavior bug was identified in STAB1A. STAB1B
normalized private test-hook keys on insert, clear, and consume paths; the
STAB1C/STAB1D audit reruns then confirmed that no remaining behavior issue was
hidden behind the missed hooks.

No failing test intentionally injected these path-scoped hooks before the
database directory existed. The failing tests opened the database first or
created the branch state first, then injected by raw temp path. STAB1B still
kept a `canonicalize()` fallback for future pre-create hook uses and for any
synthetic unit tests that intentionally use non-existent paths.

## STAB1B - Normalize Engine Fault-Injection Keys

**Goal:**

Make path-scoped engine test hooks use the same key regardless of raw temp path
versus canonical database path.

**Preferred implementation:**

- add one private test-hook key normalization helper in
  `crates/engine/src/database/test_hooks.rs`
- use it on both insert/clear and consume paths for every path-scoped hook
- preserve fallback behavior when `canonicalize()` fails, so tests that inject
  before path creation do not panic
- update failing tests to inject through `db.data_dir()` where that makes the
  intent clearer

**Acceptance:**

- unit tests cover same-path, other-path, and raw-versus-canonical lookup
  behavior for the hook key helper
- all existing hook one-shot consumption tests still pass
- hook normalization is private to test/fault-injection code

**Implementation, 2026-05-08:**

STAB1B added a private test-support `hook_path_key()` helper in
`crates/engine/src/test_path_key.rs` and routed every database path-scoped test
hook through it on insert, clear, consume, wait, and release paths. It also
routed the recipe-store seed-failure hook through the same helper so the
remaining engine path-scoped test hook uses the same key policy. The helper
canonicalizes existing paths before lexical cleanup, canonicalizes the deepest
existing parent for not-yet-created database paths, and anchors relative
pre-create paths to the canonical current directory. If canonicalization cannot
find any existing prefix, it falls back to the lexically normalized path instead
of panicking.

The runtime consume paths first check whether their hook slot is empty before
normalizing the path, so production builds do not pay filesystem
canonicalization cost when no test hook is installed.

The converted test
`database::test_hooks::tests::path_scoped_hooks_reach_canonical_aliases_after_normalization`
now proves raw aliases reach canonical consumers and raw clears remove
canonical entries for every database path-scoped hook family. The helper tests
cover canonical existing paths, distinct path separation, missing absolute and
relative paths under an existing parent, and symlink-plus-parent traversal. The
recipe-store tests cover raw/canonical alias reachability for the seed-failure
hook.

The focused shutdown suite, the two branch cleanup classification tests, and the
full `cargo test -p strata-engine` package all pass after STAB1B. STAB1C now
records the focused shutdown audit and tightens the durability-mode rollback
assertion; STAB1D remains as the branch-cleanup audit for the second original
failure cluster.

## STAB1C - Restore WAL Halt And Resume Tests

**Goal:**

Make the 8 WAL/shutdown tests deterministic and green while preserving the
halt/resume contract.

**Work:**

- rerun the focused shutdown suite after `STAB1B`
- if any test still misses the halt path, inspect whether the background flush
  thread has real dirty WAL data and whether `begin_background_sync()` can
  produce a handle at the configured interval
- replace timing-only assumptions with deterministic state waits where needed
- keep the invariant that `Halted` health and `accepting_transactions = false`
  publish together
- verify runtime durability-mode rollback restores:
  - database durability mode
  - WAL writer durability mode
  - runtime signature
  - a live, operational Standard-mode flush thread

**Acceptance:**

```bash
cargo test -p strata-engine database::tests::shutdown:: -- --nocapture
```

passes with the full shutdown test module green.

**Implementation, 2026-05-08:**

STAB1C reran the shutdown-focused failure cluster after STAB1B normalized
path-scoped test-hook keys. The missed WAL halt paths were hook reachability
failures, not missing dirty WAL data, missing background-sync handles, or a
production halt/resume bug.

The shutdown module already had deterministic state waits for halt publication,
resume blocking, failed resume accounting, and shutdown-timeout interleavings.
The only remaining loose assertion was the runtime durability-mode spawn-failure
rollback test: it checked that a live flush thread existed after rollback, but
not that the restored Standard-mode worker was operational. Because the current
runtime switch protocol stops the old Standard worker before attempting the new
configuration, same-thread identity is not a valid rollback invariant. STAB1C
now injects a sync failure after rollback and verifies that the restored
Standard-mode flush worker reaches the WAL halt path, then resumes cleanly. The
test uses a long Standard interval, appends a valid WAL record under the writer
lock, marks only the background-sync deadline as due, and unparks the restored
flush thread. That avoids the inline-sync fallback consuming the dirty WAL data
before the restored background worker can observe it.

STAB1C also tightened
`test_explicit_flush_waits_for_inflight_background_sync`. The old setup used a
normal `Database::transaction()` with a 1ms Standard durability interval, which
could trigger the WAL writer's inline-sync fallback before the test manually
called `begin_background_sync()`. The test now appends a valid WAL record
directly under the writer lock after refreshing only the inline-sync deadline,
marks the background-sync deadline as due, and calls `begin_background_sync()`
before releasing the lock. That creates the intended in-flight background sync
deterministically while still exercising
`Database::flush()` waiting on the WAL writer's public in-flight state.

The acceptance command for this section passes with the full shutdown module
green.

## STAB1D - Restore Branch Cleanup Classification

**Goal:**

Make branch cleanup fault injection reach the code paths that classify
post-commit cleanup debt.

**Work:**

- rerun the two focused branch cleanup failures after `STAB1B`
- confirm `clear_branch_storage_result()` receives the injected
  clear-branch-storage failure when rollback requests physical cleanup
- confirm `clear_default_branch_marker_if()` receives the injected marker-clear
  failure before returning `DeleteBranchCompletion`
- preserve the classification policy:
  - `DirFsync` cleanup uncertainty is a warning
  - other storage cleanup errors are `PostCommitError`
  - default-branch-marker cleanup failure is `PostCommitError`
- add or tighten assertions that the one-shot hook is consumed only on the
  intended cleanup path

**Acceptance:**

```bash
cargo test -p strata-engine \
  database::branch_mutation::tests::test_rollback_delete_true_surfaces_storage_cleanup_failure \
  -- --exact --nocapture

cargo test -p strata-engine \
  primitives::branch::index::tests::test_complete_delete_post_commit_classifies_default_marker_clear_failure \
  -- --exact --nocapture
```

both pass.

**Implementation note:**

`STAB1D` is a test-contract restoration, not a production-path rewrite. The
rollback storage cleanup and default-marker cleanup tests now assert that the
one-shot failure hooks were consumed by the intended cleanup path before
checking the returned error classification. The focused acceptance tests cover
ordinary storage cleanup and default-marker cleanup failure classification. The
existing `DirFsync` branch remains unchanged: storage cleanup uncertainty is
still reported as a warning, while ordinary storage cleanup and default-marker
cleanup failures surface as `PostCommitError`/rollback errors.

## STAB1E - Full Engine Gate And Closeout Docs

**Goal:**

Turn the engine package green and record the EG9 failure-caveat closeout.

**Required commands:**

```bash
cargo fmt --check
cargo test -p strata-engine database::tests::shutdown::
cargo test -p strata-engine \
  database::branch_mutation::tests::test_rollback_delete_true_surfaces_storage_cleanup_failure \
  -- --exact
cargo test -p strata-engine \
  primitives::branch::index::tests::test_complete_delete_post_commit_classifies_default_marker_clear_failure \
  -- --exact
cargo test -p strata-engine
cargo test -p stratadb --test storage_surface_imports
```

**Doc updates after green:**

- update [eg9-implementation-plan.md](./eg9-implementation-plan.md) to replace
  the 10-failure caveat with the stabilization commit/result
- update [engine-consolidation-plan.md](./engine-consolidation-plan.md) to state
  the engine package is green after stabilization
- keep this plan as the audit trail for why the post-EG stabilization happened

**Acceptance:**

- `cargo test -p strata-engine` passes
- EG closeout docs record the 10-failure stabilization result instead of
  treating those failures as active caveats
- no new storage-boundary, retired-crate, or optional-edge guard regressions are
  introduced
- next work item is the v1 architecture design phase

**Implementation, 2026-05-08:**

The STAB1E closeout gate is green:

- `cargo fmt --check`: passed
- `cargo test -p strata-engine database::tests::shutdown::`: passed, 44 tests
- `cargo test -p strata-engine database::branch_mutation::tests::test_rollback_delete_true_surfaces_storage_cleanup_failure -- --exact`:
  passed
- `cargo test -p strata-engine primitives::branch::index::tests::test_complete_delete_post_commit_classifies_default_marker_clear_failure -- --exact`:
  passed
- `cargo test -p strata-engine`: passed; the engine lib test binary reported
  2570 passed and 8 ignored; all integration tests passed; doc-tests reported
  69 passed and 1 ignored
- `cargo test -p stratadb --test storage_surface_imports`: passed, 44 tests

The EG9 closeout ledger and top-level engine consolidation plan now record
STAB1 as the path-key normalization plus deterministic WAL/branch cleanup
test-control fix for the original 10 shutdown/fault-injection and branch
cleanup/classification failures rather than carrying those failures as active
caveats.
