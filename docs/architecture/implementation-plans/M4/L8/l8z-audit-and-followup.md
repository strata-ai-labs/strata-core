# L8Z Audit and Follow-Up Phases

Status: **temporary working document**

This is the audit-and-phasing scratchpad for the L8Z commit hardening
slice. The original L8Z implementation and test plans (drafted before
L8Y closeout) were reviewed against the shipped codebase and found to
be partially stale, partially under-specified, and missing several
edge cases. This document captures:

1. the audit findings;
2. the 7-phase split that closes them;
3. cross-cutting concerns and risks.

Once the phases land the original plans
(`l8z-commit-hardening-pre-l9-readiness-{implementation,test}-plan.md`)
will be the source of truth. Until then this doc is the working brief.

Parent plans:
- `docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-test-plan.md`

Predecessor closeout:
- `docs/architecture/implementation-plans/M4/L8/l8y-closeout-and-followup.md`

## Audit Findings Summary

### Stale plan content

| Finding | Evidence | Fix in |
|---|---|---|
| §11 "Minimal automatic checkpoint / WAL-growth policy" is already shipped. All 14 plan tests exist in `lifecycle/tests/commit_hardening.rs` with the exact names called out. | `src/lifecycle/wal_growth.rs` (210+ LOC), `src/lifecycle/durable/maintenance.rs::evaluate_wal_growth_policy` (lines 492-578), `src/lifecycle/tests/commit_hardening.rs:20-554`. Porting log records PASS. | Phase 1 |
| "Deleted and deleting branches reject commit admission" wording is stale. Post-C Phase 1 the lifecycle enum has only Active/Deleted; the registry's transient `CommitBranchState::Deleting` is set + cleared inside a single `delete_branch` call and is not externally observable. | `src/lifecycle/branch_lifecycle.rs:17-22`; `src/commit/branch_registry.rs:19-24, 192`. | Phase 1 |
| Test plan §"Test Locations" lists `src/testkit/lifecycle/commit_hardening.rs`; this file does not exist. | `src/lifecycle/tests/commit_hardening.rs` is the real location. | Phase 1 |
| "Milestone labels are absent from Rust code" is currently false. | `src/commit/{guard.rs:63, replay.rs:79, conflict.rs:139, cache.rs:79, durable.rs:192}`; `src/branch/read.rs:1146`; `src/table/mod.rs` (8 sites); `src/testkit/{commit_runtime.rs:660, commit_runtime_runner.rs:1, commit_runtime_script.rs:1}`; `tests/{commit_runtime_closeout.rs:1, table_runtime_closeout.rs:1, branch_lsm_source_guard.rs:151,158}`. The `contains_architecture_label` scan at `tests/lifecycle_source_guard.rs:1911` only covers lifecycle paths. | Phase 2 |
| Plan duplicates many tests already shipped under different names. | `commit/tests/cache.rs:204` (`cache_commit_rejects_missing_deleted_and_stale_generation_before_allocation`), `commit/tests/durable.rs:561,571,1065,1290`, `commit/tests/conflict.rs:1-994`, `lifecycle/tests/branch_lifecycle/clear_delete.rs:655-694`, etc. | Phase 1 (cross-reference); subsequent phases (extend) |

### Ill-posed plan content

| Finding | Evidence | Fix in |
|---|---|---|
| Plan offers "two designs" for the durable gate (serialize globally vs. multiple keyed unresolved facts). Design (1) is already shipped via `active_admission: bool` + single `Option<unresolved>`. Design (2) requires removing the serialization first; the failure mode the plan worries about (second cross-branch post-WAL generic error) is structurally unreachable today. | `src/commit/durable_gate.rs:38-42, 235-256, 266-268`; `commit/tests/durable.rs:1290`. | Phase 1, Phase 3 |
| Plan §"Pre-L9 Surface Readiness" rule 1 requires `pub(crate)` default; test plan §11 #13 inspects `last_wal_growth_outcome()` which is `pub(crate)` and labels it `wal_growth_pressure_facts_are_visible_to_public_boundary`. Naming conflicts with the rule. | `src/lifecycle/tests/commit_hardening.rs:375-406`. | Phase 1 (rename) |
| Test plan §5 `recovery_replay_runs_under_exclusive_open_or_quiesce` permits either path; impl plan requires quiesce in recovery bootstrap. Bootstrap today uses exclusive-open. The disjunction lets today's behavior pass while the impl rule is violated. | `src/lifecycle/durable/bootstrap.rs` has no quiesce reference. | Phase 1 (resolve), Phase 4 (implement) |

### Real implementation gaps

| Gap | Evidence | Fix in |
|---|---|---|
| Quiesce is not integrated in clear / delete / fork / recovery bootstrap. Only checkpoint and durable close use it today. | `src/lifecycle/branch_lifecycle.rs:{578,605,640,673,745}` (no `try_begin_quiesce` references); `src/lifecycle/durable/bootstrap.rs` (no quiesce); `src/lifecycle/checkpoint.rs:1434`, `src/lifecycle/durable/close.rs:151` (already integrated). | Phase 4 |
| `commit/replay.rs` has no branch-generation check. The replay request matches `branch_id` only; a stale-generation request against a recreated branch is not caught. | `src/commit/replay.rs:75-89`. `CommitReplayRequest` / `WalRecord` carry no generation field. | Phase 5 |
| Table-manifest publication takes no generation argument; relies on the caller holding `branch_state_mut`. | `src/lifecycle/table_manifest.rs:345-353`. | Phase 5 |
| Retention / quarantine branch-scoped decisions take no generation argument. | `src/lifecycle/retention.rs` (no generation refs). | Phase 5 |
| Durable close drain re-uses a single up-front generation lookup; per-task generation is not re-validated. | `src/lifecycle/durable/close.rs:99-108`. | Phase 5 |
| Timeline-only WAL payloads are accepted. | `WalCommitPayload::new` does not require any user-mutation row. | Phase 6 |
| `mark_deleting` is `pub(crate)`, callable outside `delete_branch`, defeating the transient-state invariant. No source guard prevents misuse. | `src/commit/branch_registry.rs:192`. | Phase 3 |
| Durable close reports clean state even when `CommitUnresolvedDurableGate::unresolved` is non-empty. | `src/lifecycle/durable/close.rs:638-642`. | Phase 3 |

### Edge cases the plan misses entirely

| Edge case | Why it matters | Fix in |
|---|---|---|
| Fork timeline inheritance is unspecified. Plan rule "Branch A timeline rows must never satisfy Branch B as-of reads" (timeline §7) would break legitimate fork semantics: `from_rows` filters by `branch_id`, so Branch B forked at fork_version V has no timeline rows for T < V. | Real spec question. Either fork transcribes parent timeline rows under the child branch_id, or as-of reads escalate to the parent. | Phase 6 (decision required upfront) |
| Allocator-vs-uncertain-replay interaction. On `DurabilityUncertain` failure, the allocator IS advanced before WAL append (`commit/tests/durable.rs:1093` confirms `last_allocated == 1`). If the uncertain WAL record survives at version 1 and a later commit takes version 2, replay installs version 1 underneath. | Either the allocator must not advance on uncertain, or the gap rule for replay must be pinned. | Phase 6 |
| Same-branch read-your-writes during cache `AppliedButNotVisible`. Plan §6 only covers cross-branch leakage. | Rows are in L6 but visible is not advanced; what does `capture_read_view` on the same branch return? Not specified. | Phase 6 |
| Cache-mode commits participate in the global durable gate (`cache.rs:77` calls `durable_gate.admit_mutating_commit()`). | Plan §"Durable Gate Hardening" reads as durable-only; cache-mode subclause missing. | Phase 3 |
| Fault windows missing: panic during conflict validation; partial allocator rollback when conflict-after-allocation fails; fault during replay's `replace_exact` path; partial WAL records mid-record during replay idempotency. | Plan lists 15 fault tests but these are absent. | Phase 7 |
| Recovery is a lifecycle state, not concurrent with normal commits. Plan's `automatic_checkpoint_deferred_while_recovery_in_progress` implies concurrency; in fact the deferral comes from `LifecycleStateMachine::admit`. | Test name is fine but plan prose implies concurrent execution. | Phase 1 (clarify prose) |
| Fuzz inventory drift. `tests/commit_runtime_fuzz_inventory.rs::COMMIT_RUNTIME_FUZZ_TARGETS` is a 4-element array; `commit_runtime_closeout.rs:268` checks pairwise distinctness against it. Adding plan's 5 new targets silently breaks closeout. | Plan does not list updating either file as an impl step. | Phase 7 |
| `set_parent_for_recovery` (`branch_lifecycle.rs:319`) mutates descriptor with no generation guard; exclusivity documented only by comment. | Edge case for replay-after-recreate scenarios. | Phase 5 |
| Architecture-label scan does not cover fuzz-corpus bytes or runtime-constructed error strings. | Plan says these are in scope; current scanner only handles source bytes. | Phase 2 (decide drop or extend) |

## Phase Plan

The audit work is split into 7 phases. Phase 1 must land first so the
remaining phases target the corrected spec. Phases 2-7 may interleave
with other work but each closes one cross-cutting subsystem.

Per-phase LOC estimates are net change including tests; each is below
the 1500-LOC slice cap.

### Phase 1: Plan corrections (docs only)

Goal: align the L8Z planning docs with the shipped codebase and the
audit findings. No code changes.

Scope:

1. Remove §11 (WAL-growth policy) from the impl plan's "Implementation
   Steps" and mark it as shipped in the deferred-work / status table.
   Test plan §11 stays as a "verify shipped" matrix.
2. Rewrite §"Durable Gate Hardening" to commit to the single-admission
   design. Reframe rule 1 as a structural property to assert, not a
   bug to fix. Document the cross-branch admission lock semantics.
3. Replace "Deleted and deleting branches reject commit admission"
   with "Deleted lifecycle branches reject commit admission;
   `CommitBranchState::Deleting` is transient and not externally
   observable".
4. Fix `src/testkit/lifecycle/commit_hardening.rs` to
   `src/lifecycle/tests/commit_hardening.rs` in §"Test Locations".
5. Replace each duplicate-name test in the test plan with a cross-
   reference to the existing test (e.g., §2.1
   `cache_commit_rejects_stale_generation` → "extend
   `cache_commit_rejects_missing_deleted_and_stale_generation_before_allocation`
   with phase classification assertions").
6. Resolve test §5 `recovery_replay_runs_under_exclusive_open_or_quiesce`
   into one of: rename to `..._under_exclusive_open` (matches today)
   or `..._under_quiesce` (mandates Phase 4 wiring).
7. Rename `wal_growth_pressure_facts_are_visible_to_public_boundary` to
   `wal_growth_pressure_facts_have_stable_observation_api`.
8. Clarify recovery-vs-concurrent-commits prose: deferral is via
   `LifecycleStateMachine::admit`, not concurrent execution.
9. Add a §"Cache Mode" subclause to durable gate hardening describing
   cache-mode participation in `admit_mutating_commit`.
10. Add a §"Fork Timeline Inheritance" question to the open-questions
    block, deferring the decision to Phase 6 plan mode.

Estimated change: ~600 LOC across two docs.

Exit gate: both plans pass a coherence read; all 14 stale entries
above are corrected; no plan section claims work that is already
shipped without marking it shipped.

### Phase 2: Milestone-label sweep + source-guard widening

Goal: get the codebase to the state the L8Z plan claims it is in.

Scope:

1. Delete or rephrase milestone-label comments in:
   - `src/commit/{guard.rs:63, replay.rs:79, conflict.rs:139, cache.rs:79,108, durable.rs:192}`
   - `src/branch/read.rs:1146`
   - `src/table/mod.rs` (8 `reason = "...M4 table slices"` lints)
   - `src/testkit/{commit_runtime.rs:660, commit_runtime_runner.rs:1, commit_runtime_script.rs:1}`
   - `tests/{commit_runtime_closeout.rs:1, table_runtime_closeout.rs:1, branch_lsm_source_guard.rs:151,158}`
2. Move `contains_architecture_label` from
   `tests/lifecycle_source_guard.rs:1911` into a shared helper module
   under `tests/common/` (or replicate at `tests/commit_runtime_source_guard.rs`).
3. Add a `commit_runtime_implementation_avoids_architecture_labels`
   test to `tests/commit_runtime_source_guard.rs` scanning
   `src/commit/`, `src/branch/`, `src/testkit/commit_runtime*.rs`,
   `tests/commit_runtime_*.rs`, `tests/commit_runtime_closeout.rs`,
   `tests/branch_lsm_*.rs`.
4. Decide on fuzz-corpus byte scanning and runtime-error-string
   scanning. Either drop those clauses from the test plan (revisit
   Phase 1) or add a corpus scanner under `tests/`.

Estimated change: ~700 LOC (mostly small per-file edits; some test
additions).

Exit gate: `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
and `--test lifecycle_source_guard` pass with the widened scan; no
`L[1-9]` / `M[0-9]` labels remain in scanned source.

### Phase 3: Durable gate + close-clean + cache-mode interaction

Goal: lock in the single-admission durable-gate design and close the
remaining gate-state observability gaps.

Scope:

1. Replace `mark_deleting`'s `pub(crate)` with `pub(super)` (or move
   it into a private helper of `delete_branch`) so it cannot be
   called outside the delete path. Add a source-guard test asserting
   the symbol is not re-exported.
2. In `lifecycle/durable/close.rs`, extend the clean-state check at
   lines 638-642 to also require
   `commit_runtime.unresolved_durable_gate().unresolved.is_none()`.
   Return a typed `CloseAbortedUnresolvedDurable` error otherwise.
3. Add the §"Cache Mode" tests called for by Phase 1's plan edit:
   `cache_commit_observes_global_durable_admission_lock`,
   `cache_record_unresolved_uses_not_durable_class`.
4. Add the structural cross-branch tests called for by reframed
   gate rule 1: `cross_branch_second_admission_blocks_at_active_admission`
   asserts the second branch never reaches `record_unresolved`.
5. Tighten the gate error variant for sequential same-branch
   `record_unresolved` mismatches to a phase-specific error code
   (separate from the structurally-unreachable cross-branch case).
   Update `commit/tests/durable_gate.rs:369-405` to match.

Estimated change: ~1000 LOC (300 src, 700 tests).

Exit gate: gate has a single shipped design, `mark_deleting` is
non-pub-crate, durable close rejects clean-state report on residual
gate, cache-mode participation is tested.

### Phase 4: Quiesce integration

Goal: every branch lifecycle operation and recovery boundary holds
quiesce while it captures or replaces state.

Scope:

1. Wire `try_begin_quiesce` into:
   - `branch_lifecycle.rs::clear_branch` (currently at line 578)
   - `branch_lifecycle.rs::delete_branch` (line 605)
   - `branch_lifecycle.rs::fork_current` (line 640)
   - `branch_lifecycle.rs::fork_at_retained_version` (line 673)
   - `branch_lifecycle.rs::fork_at_retained_timestamp` (line 745)
   - `lifecycle/durable/bootstrap.rs::complete_recovery` (or the
     replay loop) per Phase 1's resolution of test §5.
2. Quiesce must be released on every error path (RAII via
   `CommitQuiesceToken`'s Drop). Verify under failure injection.
3. Add the 8 quiesce tests called for by test §5, including:
   - `clear_uses_quiesce_before_state_swap`
   - `delete_uses_quiesce_before_release_facts`
   - `fork_uses_quiesce_before_source_capture`
   - `recovery_replay_runs_under_quiesce` (or `_exclusive_open`)
   - `quiesce_guard_releases_on_branch_lifecycle_failure`

Estimated change: ~1100 LOC (400 src, 700 tests).

Exit gate: every required quiesce user invokes
`try_begin_quiesce`; tests cover release-on-failure for each site.

### Phase 5: Generation guard plumbing

Goal: every operation that crosses a queue, durable boundary, or
recreated-branch window carries or validates branch generation.

Scope:

1. Thread `branch_generation: CommitBranchGeneration` into
   `CommitReplayRequest`. Decide WAL format change:
   - Option A: add `branch_generation` field to `WalRecord`
     (format version bump, golden vectors needed). Discuss with
     user before proceeding — this touches frozen-format territory.
   - Option B: derive generation from catalog at replay dispatch
     time (no WAL change), use the catalog's current generation as
     the guard. Acceptable if `replay_branch_catalog_manifest`
     already runs before WAL replay (it does, per B Phase 2).
2. Add a generation argument to
   `lifecycle/table_manifest.rs::publish_table_manifest_for_branch`.
3. Add a generation argument to `lifecycle/retention.rs` branch-
   scoped decisions (`retention/quarantine` paths).
4. In `lifecycle/durable/close.rs:99-108`, re-validate generation
   per drained task instead of a single up-front lookup.
5. Add generation enforcement to
   `branch_lifecycle.rs::set_parent_for_recovery` (or move it
   behind a recovery-only path that proves exclusivity).
6. Add the 12 §2 tests called for by the corrected test plan,
   pointing each at the existing stale-generation tests they
   extend (per Phase 1's cross-references).
7. Add a `no_generation_paths_are_exclusive_and_documented` test
   that scans the source for callers of guard-free helpers and
   asserts each has an inline rationale.

Estimated change: ~1400 LOC (600 src, 800 tests).

Exit gate: every surface in impl §"Branch Generation Guard
Coverage" has either an explicit generation argument or an
inline rationale tagged with a documented exclusivity proof.

### Phase 6: Timeline + visibility edge cases

Goal: pin the remaining read-side correctness questions.

**Open question at start of phase**: fork timeline inheritance.
Phase mode starts with `AskUserQuestion` to choose:
- Option A: child branch transcribes parent timeline rows under
  child branch_id at fork time. Cost: more storage and an
  encoder pass at fork. Benefit: as-of reads work with no
  parent-pointer chase.
- Option B: as-of reads on a forked branch consult the parent
  when `T < fork_version`. Cost: read-path complexity. Benefit:
  zero fork-time overhead.

Scope (once fork question is decided):

1. Reject timeline-only WAL payloads in `WalCommitPayload::new`
   or at replay validation. Add `timeline_only_wal_payload_rejects`.
2. Implement the chosen fork-timeline inheritance approach. Add
   `forked_branch_as_of_read_returns_inherited_history` and
   `forked_branch_isolated_from_parent_after_fork_version`.
3. Pin same-branch read-your-writes under `AppliedButNotVisible`:
   `capture_read_view` on the same branch after a cache-mode
   visibility failure must NOT see the applied rows (visible
   bound caps them). Add
   `same_branch_read_after_applied_not_visible_is_invisible_bounded`.
4. Pin allocator-vs-uncertain-replay interaction. Decision: the
   allocator does advance on uncertain failure (already the
   shipped behavior). Document the contract: a surviving uncertain
   WAL record at version V is replayed; the live runtime may have
   already advanced past V; replay installs V's rows in L6
   underneath; visible-version advancement is gated on the
   replay completing. Add `uncertain_wal_record_replays_below_live_allocator`.
5. Add the remaining §6 / §9 tests from the corrected test plan.

Estimated change: ~1400 LOC (700 src, 700 tests). Higher if
Option A is chosen for fork inheritance.

Exit gate: fork inheritance decision documented; timeline-only
WAL rejected; read-your-writes pinned; uncertain-replay contract
pinned.

### Phase 7: Fault windows + new fuzz targets + Q-Z closeout

Goal: close the assurance layer.

Scope:

1. Add the 15 §"Fault Windows" tests, including the four the
   audit flagged as missing:
   - `fault_during_replay_gate_replace_exact`
   - `fault_during_conflict_validation_panic_safe`
   - `fault_after_allocation_partial_rollback`
   - `fault_during_replay_partial_wal_record`
2. Add two new fuzz targets:
   - `commit_hardening_quiesce` (no existing coverage)
   - `commit_hardening_checkpoint_policy` (no existing coverage)
3. Drop the three plan-listed targets that duplicate existing
   coverage (`commit_hardening_admission`,
   `commit_hardening_durable_gate`,
   `commit_hardening_replay_timeline`); document the overlap
   with `commit_runtime_{batch,conflict,durable,timeline}` in
   the closeout doc instead.
4. Update `tests/commit_runtime_fuzz_inventory.rs::COMMIT_RUNTIME_FUZZ_TARGETS`
   to the 6-element list. Update `commit_runtime_closeout.rs:268`
   pairwise-distinctness assertion to match.
5. Add the 8 §"Q-Z Closeout Tests" referencing real inventory
   (source files, fuzz targets, sensitivity-probe ledger,
   command matrix). Reuse the closeout pattern from
   `lifecycle_closeout.rs` and `commit_runtime_closeout.rs`.
6. Update the porting log L8Z section with command-matrix
   outcomes and the sensitivity-probe ledger.

Estimated change: ~1300 LOC (400 src, 900 tests).

Exit gate: every plan-listed fault window has a test; fuzz
inventory matches closeout assertion; Q-Z closeout tests scan
real inventory, not planning-doc presence.

## Cross-Cutting Concerns

### WAL format stability

Phase 5 carries the only format-change risk (option A of step 1:
adding `branch_generation` to `WalRecord`). The M3 freeze gate means
this needs reviewer approval and golden-vector regeneration. Prefer
option B (catalog-derived generation at replay dispatch) unless the
WAL-on-disk record itself needs to carry generation for cross-process
recovery — and storage-next single-process is the V1 boundary.

### Closeout array sync

Phase 7's `COMMIT_RUNTIME_FUZZ_TARGETS` change must land atomically
with the closeout pairwise-distinctness check. Splitting these across
two commits would break CI in between.

### Phase ordering

Phases 2-7 may interleave but the listed order is intentional:

- Phase 2 lands the source-guard widening that subsequent phases
  must not regress.
- Phase 3 closes the gate semantics that Phase 4's quiesce work and
  Phase 7's fault tests build on.
- Phase 4 (quiesce) must precede Phase 5 (generation guards on close-
  drain) because per-task validation depends on the close path
  already quiescing.
- Phase 5 (generation) must precede Phase 6 (fork inheritance) if
  Option A is chosen, because fork transcription must run under a
  generation guard.
- Phase 7 closes everything else.

### Risks

| Risk | Mitigation |
|---|---|
| Phase 1 doc edits drift from Phase 2-7 reality. | Re-read the doc at the start of each subsequent phase; treat any drift as a Phase 1 fix-up before continuing. |
| Phase 4 quiesce introduces deadlock between branch guard and quiesce token. | Existing tests in `commit/tests/guard.rs:94-317` already cover the ordering; add release-under-failure tests in Phase 4 explicitly. |
| Phase 5 WAL-format option A blocks on M3 freeze review. | Default to option B; only escalate if a cross-process recovery requirement surfaces. |
| Phase 6 fork-inheritance decision changes behavior for existing fork callers. | Run the full `lifecycle_branch_lifecycle` integration suite before and after; treat any silent semantic change as a regression. |
| Phase 7 closeout array split breaks CI mid-merge. | Land in one commit. |

## Out of Scope

These remain outside L8Z and its phases:

- Public storage API surface (L9 owns it).
- Public transaction sessions, transaction IDs, cross-branch atomic
  commits, distributed coordination, remote sync.
- Product branch workflows (merge / cherry-pick / revert).
- Physical format freeze, backward compatibility, migration tooling.
- Background scheduler / adaptive maintenance policy beyond the
  minimal WAL-growth trigger (already shipped).
- The original L8Z plan's §"Generated Model Tests" beyond the
  hidden-applied / WAL-growth-threshold extensions. The bulk of the
  generated-model framework is already in
  `testkit/commit_runtime_model.rs`.

## Test Count Deltas

Tracking against the original test plan's required-test counts:

| Section | Original required | Already shipped (different name) | Net new in phases | Notes |
|---|---|---|---|---|
| §1 Transaction-id | 8 | 5-6 | 2-3 | Cross-references in Phase 1 |
| §2 Generation guard | 12 | 6 | 6 | Phase 5 |
| §3 Conflict | 12 | 9-10 | 2-3 | Phase 1 cross-refs |
| §4 Concurrency | 10 | 4 | 6 | Phase 3, 4 |
| §5 Quiesce | 12 | 4 | 8 | Phase 4 |
| §6 Visibility | 10 | 1-2 | 7-8 | Phase 6 |
| §7 Durable gate | 10 | 4 | 6 | Phase 3 |
| §8 Durability-uncertain | 10 | 2 | 8 | Phase 6 |
| §9 Timeline | 12 | 6 | 6 | Phase 6 |
| §10 Outcome | 10 | 6 | 4 | Phase 7 |
| §11 WAL-growth | 14 | 14 | 0 | Already shipped |
| §"Fault Windows" | 15 | 0 | 15 | Phase 7 |
| §"Source Guards" | 7 | 4-5 | 2-3 | Phase 2 |
| §"Q-Z Closeout" | 8 | 0 | 8 | Phase 7 |
| **Total** | **150** | **65-71** | **79-86** | |

These numbers are estimates; per-phase plan mode will refine them.

## Open Questions

1. Phase 5 step 1: WAL format change for `branch_generation` (option
   A) or catalog-derived at replay (option B)? Default: B.
2. Phase 6 fork timeline inheritance: transcribe at fork (A) or
   parent-chase at read (B)? No default; needs explicit decision.
3. Phase 2 step 4: drop fuzz-corpus byte scanning and runtime-error-
   string scanning from the test plan, or add scanners? Default:
   drop (these are not load-bearing for the V1 surface).
4. Phase 7 step 3: drop the three duplicate fuzz targets or keep
   them as aliases pointing at the existing targets? Default: drop.
