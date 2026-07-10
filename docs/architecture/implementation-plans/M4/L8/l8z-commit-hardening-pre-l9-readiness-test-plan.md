# L8Z Test Plan: Commit Hardening And Pre-L9 Readiness

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that the storage commit runtime is ready for the public API layer: no
hidden transaction-id assumptions, complete branch-generation protection,
correct conflict/concurrency behavior, sound quiesce integration, explicit
durable uncertainty, safe global visibility, and closeout coverage for the
durable-table/storage-hardening work. It also proves V1 has a minimal
storage-owned checkpoint/WAL-growth policy.

The suite must fail if L8Z:

1. reintroduces V1 transaction ids without recovery catch-up;
2. lets stale branch generations mutate state or publish objects;
3. allows a commit during quiesce;
4. leaks branch guards or quiesce guards on error;
5. misclassifies cross-branch post-WAL failures;
6. lets applied-not-visible rows become visible by side effect;
7. drops durability-uncertain WAL facts;
8. accepts timeline-only WAL payloads;
9. lets durable WAL grow past configured bounds without checkpoint pressure or
   health facts;
10. truncates WAL without a checkpoint/table-manifest retention proof;
11. returns product transaction wording from storage errors;
12. adds milestone labels to Rust code, test names, fixture bytes, or
    user-facing error strings.

Do not add tests whose only assertion is that planning documents exist or link
to other planning documents.

## Coverage Boundary

Covered:

1. transaction-id absence/deferral;
2. branch-generation guard coverage;
3. read-set and CAS validation windows;
4. same-branch and cross-branch concurrency;
5. commit quiesce;
6. cache and durable visibility safety;
7. durable gate and durability-uncertain outcomes;
8. replay and timeline hardening;
9. outcome validation;
10. minimal automatic checkpoint/WAL-growth policy;
11. source guards and Q-Z closeout.

Not covered:

1. public API methods;
2. public transaction sessions;
3. distributed commits;
4. cross-branch atomic product operations;
5. product branch workflows;
6. remote/hub sync;
7. query-layer behavior.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `storage/src/txn/manager.rs` | Version allocation, visibility publication, branch locks, quiesce, pending-version barriers. | Commit versions/timestamps catch up; quiesce/guard/visibility facts remain sound. |
| `storage/src/txn/context.rs` | Internal staged write facts and validation inputs. | Commit batches validate storage mutations without public transaction sessions. |
| `storage/src/txn/validation.rs` | Read-set and CAS checks. | Stale read/CAS facts reject before allocation and mutation. |
| `storage/src/txn/lock_ordering.rs` | Lock-order discipline prevents deadlocks. | Guard acquisition order is documented and tested under failures. |
| `storage/src/durability/commit_adapter.rs` | WAL-before-visible and ambiguous durability handling. | Durable failure matrix keeps phase-specific outcomes. |
| `engine/src/database/transaction.rs` | Writer health and branch-generation barriers. | Storage exposes raw facts without product observer hooks. |

Tests must not port:

1. public begin/commit/rollback sessions;
2. durable transaction ids;
3. product timeout/session metrics;
4. engine observer callbacks;
5. user-facing ACID wording;
6. remote branch publish semantics.

## Test Locations

Use:

1. `crates/storage-next/src/commit/tests/allocator.rs`
2. `crates/storage-next/src/commit/tests/batch.rs`
3. `crates/storage-next/src/commit/tests/branch_registry.rs`
4. `crates/storage-next/src/commit/tests/cache.rs`
5. `crates/storage-next/src/commit/tests/conflict.rs`
6. `crates/storage-next/src/commit/tests/durable.rs`
7. `crates/storage-next/src/commit/tests/durable_gate.rs`
8. `crates/storage-next/src/commit/tests/guard.rs`
9. `crates/storage-next/src/commit/tests/outcome.rs`
10. `crates/storage-next/src/commit/tests/replay.rs`
11. `crates/storage-next/src/commit/tests/timeline.rs`
12. `crates/storage-next/src/lifecycle/tests/branch_lifecycle.rs`
13. `crates/storage-next/src/lifecycle/tests/recovery.rs`
14. `crates/storage-next/src/testkit/commit_runtime.rs`
15. `crates/storage-next/src/lifecycle/tests/commit_hardening.rs`
16. `crates/storage-next/tests/commit_runtime_properties.rs`
17. `crates/storage-next/tests/commit_runtime_faults.rs`
18. `crates/storage-next/tests/lifecycle_closeout.rs`
19. `crates/storage-next/tests/lifecycle_source_guard.rs`
20. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md`

Split files before they exceed 1,000 lines.

## Direct Unit Tests

### 1. Transaction-Id Policy

Required tests:

1. `commit_runtime_has_no_v1_transaction_id_allocator`
2. `wal_commit_payload_has_no_transaction_id_field`
3. `replay_catches_up_commit_version_without_transaction_id`
4. `bootstrap_catches_up_timestamp_without_transaction_id`
5. `source_guard_blocks_transaction_id_reintroduction` —
   *Existing: `crates/storage-next/tests/commit_runtime_source_guard.rs`
   `commit_runtime_source_guard_catches_product_vocabulary`. Extend with an
   assertion that the `TransactionId` / `TxnId` lifecycle-side ban scans
   lifecycle source roots.*
6. `deferred_map_records_transaction_id_policy`
7. `commit_outcome_uses_commit_version_as_ordering_identity`
8. `old_transaction_context_terms_do_not_appear_in_user_errors`

Assertions:

1. V1 commit ordering is commit-version based;
2. transaction-id references are absent from code or explicitly deferred in
   docs/source guards;
3. recovery needs no transaction-id allocator catch-up.

### 2. Branch Generation Guard Coverage

Required tests:

1. `cache_commit_rejects_stale_generation` —
   *Existing: `crates/storage-next/src/commit/tests/cache.rs:204`
   `cache_commit_rejects_missing_deleted_and_stale_generation_before_allocation`.
   Extend with a phase-classification assertion that the rejection happens before
   `admit_mutating_commit` advances the gate or allocator.*
2. `durable_commit_rejects_stale_generation_before_wal_append` —
   *Existing: `crates/storage-next/src/commit/tests/durable.rs` covers
   the durable-mode stale-generation rejection at branch admission;
   `commit/branch_registry.rs::validate_target` validates the guard
   before WAL append.*
3. `replay_rejects_stale_generation` —
   *Deferred to a future slice (post-Phase 5). Phase 5 plan mode
   considered three options: (D) recovery-time `created_at` filter
   without WAL format change, (A) `branch_generation` field added to
   `WalRecord` with a format-version bump, and (C) defer the safety
   fix to a future slice. Option D turned out to require enforcing
   strict semantics on `created_at` that 15+ existing call sites
   violate (caller-controlled label, not a strict commit-version
   bound). Option A requires a WAL format change. Both have similar
   LOC budgets; the user chose option C, deferring the fix to a
   dedicated future slice that picks one path and lands it without
   the Phase 5 multi-step bundle. The gap is documented in the L8Z
   porting log under "Deferred replay-safety gap"; the catalog
   manifest is authoritative for the live generation but stale
   pre-recreate WAL records below the live `created_at` are silently
   applied to live state until the dedicated slice ships.*
4. `flush_rejects_stale_generation_before_table_build` —
   *Existing: `crates/storage-next/src/lifecycle/tests/branch_lifecycle/clear_delete.rs:680`
   `stale_flush_task_generation_rejects_after_recreate` exercises the
   rejection via `replace_active_branch_state_with_descriptor`
   returning `BranchNotWritable` for a stale descriptor — the
   table-build can never reach publication.*
5. `compaction_rejects_stale_generation_before_output_publish` —
   *Existing: `crates/storage-next/src/lifecycle/tests/branch_lifecycle/clear_delete.rs:685`
   `stale_compaction_task_generation_rejects_after_recreate`. Same
   `BranchNotWritable` rejection path as flush.*
6. `materialization_rejects_stale_generation_before_handle_bind` —
   *Existing: `crates/storage-next/src/lifecycle/tests/branch_lifecycle/clear_delete.rs:690`
   `stale_materialization_task_generation_rejects_after_recreate`.
   Same rejection path.*
7. `checkpoint_rejects_stale_generation_when_targeting_branch_state` —
   *Covered by the same `replace_active_branch_state_with_descriptor`
   rejection path. Checkpoint row capture acquires `branch_state_mut`
   with the captured generation guard; the catalog rejects on
   mismatch before any row capture happens.*
8. `row_pruning_rejects_stale_generation_before_rewrite` —
   *Covered by the same `BranchNotWritable` rejection path. Row pruning
   runs inside compaction; the compaction stale-rejection test
   `stale_compaction_task_generation_rejects_after_recreate` already
   covers the pruning surface.*
9. `branch_lifecycle_rejects_stale_generation` —
   *Existing: `crates/storage-next/src/lifecycle/tests/branch_lifecycle/clear_delete.rs:655`
   `stale_commit_generation_rejects_after_recreate` and the umbrella
   helper `stale_rewrite_generation_rejects_after_recreate` exercise
   the branch-lifecycle wrappers' guard validation.*
10. `close_drain_rejects_stale_generation_work` —
    *Structurally guarded by Phase 4's quiesce wiring on branch-
    lifecycle wrappers. Recreate cannot run during close (close
    quiesces; recreate fails on quiesce token). Tasks enqueued
    pre-close have their captured-generation validated when the close
    drain calls `branch_state_mut` with the up-front lookup. The
    audit's concern that a "task captured at gen-1 against live
    gen-2 state" silently corrupts is addressed by the existing
    `BranchNotWritable` / `BranchGenerationMismatch` rejection at
    branch-state acquisition (existing
    `stale_flush_task_generation_rejects_after_recreate` test
    covers the closest scenario).*
11. `generation_mismatch_error_reports_expected_and_actual` —
    *Existing: `crates/storage-next/src/commit/branch_registry.rs:76-90`
    `CommitBranchGenerationGuard::validate` returns
    `CommitRuntimeError::BranchGenerationMismatch { branch_id,
    expected, actual }`. The existing
    `stale_commit_generation_rejects_after_recreate` test
    (`clear_delete.rs:669`) asserts on `expected` and `actual` values.*
12. `no_generation_paths_are_exclusive_and_documented` —
    *Phase 5: `tests/lifecycle_source_guard.rs::recovery_exclusivity_token_is_minted_only_in_bootstrap`.
    A future slice may extend this with a broader scan of `pub(crate)
    fn ... (&mut self, branch_id: BranchId, ...)` signatures that
    omit a `CommitBranchGenerationGuard`; the
    `RecoveryExclusivityToken` pattern (see
    `lifecycle/branch_lifecycle.rs::set_parent_for_recovery`) is the
    template for compile-time-enforceable non-guarded paths.*

Assertions:

1. stale generation rejects before durable side effects;
2. every no-generation caller has an explicit exclusivity proof;
3. deleted lifecycle branches reject commit admission
   (`LifecycleBranchStatus::Deleted`);
4. `CommitBranchState::Deleting` is transient inside `delete_branch` and not
   externally observable.

### 3. Conflict Validation

*Note: 9-10 of the tests below already exist in
`crates/storage-next/src/commit/tests/conflict.rs` (lines 1-994) under
different names (`branch_read_view_conflict_source_can_be_capped_at_visible_version`,
`read_set_present_facts_match_or_report_storage_conflict`, and similar).
Phase 5 plan mode will pair each L8Z-listed name with its existing counterpart
and decide whether to rename, alias, or extend with new assertions. The new
required-test items (`conflict_failure_does_not_advance_allocator`,
`conflict_failure_does_not_change_visible_version`,
`duplicate_conflict_facts_are_rejected_or_normalized`) remain net-new.*

Required tests:

1. `read_set_conflict_rejects_before_allocation`
2. `cas_conflict_rejects_before_allocation`
3. `blind_write_allows_missing_read_set`
4. `read_set_uses_captured_visible_bound`
5. `read_set_does_not_see_unpublished_same_branch_rows`
6. `read_set_does_not_use_other_branch_rows`
7. `cas_uses_target_branch_only`
8. `conflict_failure_releases_branch_guard`
9. `conflict_failure_does_not_advance_allocator`
10. `conflict_failure_does_not_change_visible_version`
11. `conflict_source_documents_single_process_assumption`
12. `duplicate_conflict_facts_are_rejected_or_normalized`

Assertions:

1. validation happens before allocation/mutation;
2. guards are released on every failure path;
3. branch isolation is explicit.

### 4. Concurrency And Guard Ordering

Required tests:

1. `same_branch_commits_are_serialized`
2. `same_branch_second_commit_gets_guard_unavailable_when_guard_held`
3. `cross_branch_cache_commits_do_not_share_branch_guard`
4. `cross_branch_durable_commits_preserve_global_visibility_safety`
5. `guard_acquisition_order_is_stable`
6. `guard_release_happens_after_visibility_publication`
7. `guard_release_happens_after_durable_gate_recording_on_failure`
8. `poisoned_guard_lock_returns_typed_error`
9. `guard_drop_releases_after_panic_safe_boundary`
10. `generated_interleavings_do_not_deadlock`

Assertions:

1. same-branch serialization is structural;
2. cross-branch independence does not violate global visible/durable facts;
3. lock-order comments and tests match implementation.

### 5. Quiesce Integration

Required tests:

1. `quiesce_rejects_while_branch_guard_active` —
   *Existing: `src/commit/tests/guard.rs:119`
   `quiesce_cannot_start_with_active_guards_or_while_already_active`.*
2. `branch_guard_rejects_while_quiesce_active` —
   *Existing: `src/commit/tests/guard.rs:94`
   `quiesce_blocks_new_mutating_guards_until_token_drops`; also
   `src/lifecycle/tests/durable.rs:1324`
   `cross_branch_commit_after_quiesce_rejects`.*
3. `checkpoint_uses_quiesce_before_row_capture` —
   *Shipped: `src/lifecycle/checkpoint.rs:1434` acquires
   `try_begin_quiesce` before row capture; existing tests
   `src/lifecycle/tests/checkpoint.rs:182, 213`.*
4. `fork_uses_quiesce_before_source_capture` —
   *Phase 4. Covered by six new tests (three fork variants × two
   modes):
   `src/lifecycle/tests/durable.rs::durable_fork_{current,at_retained_version,at_retained_timestamp}_requires_quiesce_and_rejects_when_branch_guard_active`
   and
   `src/lifecycle/tests/cache.rs::cache_fork_{current,at_retained_version,at_retained_timestamp}_requires_quiesce_and_rejects_when_branch_guard_active`.*
5. `clear_uses_quiesce_before_state_swap` —
   *Phase 4. Covered by
   `src/lifecycle/tests/durable.rs::durable_clear_branch_requires_quiesce_and_rejects_when_branch_guard_active`
   and
   `src/lifecycle/tests/cache.rs::cache_clear_branch_requires_quiesce_and_rejects_when_branch_guard_active`.*
6. `delete_uses_quiesce_before_release_facts` —
   *Phase 4. Covered by
   `src/lifecycle/tests/durable.rs::durable_delete_branch_requires_quiesce_and_rejects_when_branch_guard_active`
   and
   `src/lifecycle/tests/cache.rs::cache_delete_branch_requires_quiesce_and_rejects_when_branch_guard_active`.*
7. `recovery_replay_runs_under_exclusive_open` —
   *Locked by Phase 1 (Open Questions §A). Recovery bootstrap relies
   on exclusive open + `LifecycleStateMachine::admit` gating.*
8. `close_uses_quiesce_before_final_sync` —
   *Shipped: `src/lifecycle/durable/close.rs:151` acquires
   `try_begin_quiesce` before WAL sync; existing test
   `src/lifecycle/tests/durable.rs:1283`
   `quiesce_blocks_new_branch_guards_until_close_completes`.*
9. `quiesce_guard_releases_on_checkpoint_failure` —
   *Shipped: `src/lifecycle/tests/checkpoint.rs:213`
   `checkpoint_snapshot_publish_failure_releases_quiesce_and_keeps_recovery_facts`.*
10. `quiesce_guard_releases_on_branch_lifecycle_failure` —
    *Phase 4. Covered by
    `src/lifecycle/tests/durable.rs::branch_lifecycle_quiesce_guard_releases_on_failure_so_followup_acquire_succeeds`.*
11. `quiesce_guard_releases_on_close_failure` —
    *Shipped: `src/lifecycle/tests/durable.rs:1294`
    `quiesce_guard_released_on_retryable_failure_when_contract_allows_retry`.*
12. `quiesce_error_preserves_source_code`

Assertions:

1. no commit enters while quiesce is active;
2. quiesce release is RAII and failure-safe;
3. quiesce has no durability/visibility side effects by itself.

### 6. Global Visibility Safety

Required tests:

1. `cache_applied_not_visible_blocks_cross_branch_visible_advance` —
   *Existing: `src/commit/tests/cache.rs:935`
   `cache_commit_rejects_any_unresolved_durable_gate_before_allocation`
   verifies cross-branch cache commits are blocked by the unresolved
   global gate set up by a prior `applied_not_visible` recording.*
2. `cache_applied_not_visible_recovery_path_reports_health_debt` —
   *Existing: `src/lifecycle/tests/cache.rs` health-debt path covers
   the recovery-side reporting.*
3. `durable_applied_not_visible_blocks_cross_branch_visible_advance` —
   *Existing: `src/commit/tests/durable.rs:1290`
   `durable_active_global_admission_blocks_other_branch_before_wal_append`
   covers the broader cross-branch admission gate that subsumes the
   applied-not-visible block.*
4. `hidden_lower_version_row_cannot_become_readable_by_side_effect` —
   *Phase 6 pin (same-branch RYW): `src/commit/tests/cache.rs::cache_applied_not_visible_row_is_visible_to_same_branch_read_your_writes`
   pins the same-branch RYW contract; the cross-branch half is item 1.*
5. `resolving_hidden_row_unblocks_visible_advance` —
   *Existing: `src/commit/tests/durable_gate.rs` covers resolution
   sequences via `clear_exact`.*
6. `visible_tracker_rejects_publish_past_hidden_applied_row` —
   *Existing: `src/commit/tests/visibility.rs` covers monotonicity.*
7. `visible_tracker_preserves_global_monotonicity` —
   *Existing: `src/commit/tests/visibility.rs`.*
8. `branch_local_max_does_not_override_global_visible_gate` —
   *Existing: `src/commit/cache.rs::require_branch_not_ahead_of_visible`
   gate enforces this; covered by
   `src/commit/tests/cache.rs::cache_commit_rejects_allocator_visible_mismatch_before_apply`.*
9. `close_reports_unresolved_hidden_applied_row` —
   *Existing: `src/lifecycle/tests/durable.rs:675`
   `durable_close_does_not_report_complete_with_unresolved_durable_gate`.*
10. `recovery_replays_hidden_applied_row_before_visible_catchup` —
    *Existing: `src/commit/tests/replay.rs` covers gate-clearing
    replay paths; `src/lifecycle/tests/recovery.rs::recovery_rebuilds_active_branch_states`
    verifies the row-rebuild + visible-catchup ordering.*

Assertions:

1. applied-not-visible is never silently converted into visible;
2. global visible advancement is blocked or reconciled explicitly.

### 7. Durable Gate And Phase Classification

Required tests:

1. `durable_post_wal_apply_failure_records_durable_not_applied`
2. `durable_post_apply_visible_failure_records_applied_not_visible`
3. `cross_branch_two_post_wal_failures_preserve_both_classifications` —
   *Structurally unreachable per impl plan §"Durable Gate Hardening" rule 1
   (single-admission lock blocks the second branch from reaching
   `record_unresolved`). The existing
   `crates/storage-next/src/commit/tests/durable.rs:1290`
   `durable_active_global_admission_blocks_other_branch_before_wal_append`
   verifies the structural property: the second branch is rejected before
   WAL append, so two cross-branch post-WAL failures cannot occur.*
4. `cross_branch_second_failure_does_not_return_generic_gate_error` —
   *Structurally unreachable per impl plan §"Durable Gate Hardening" rule 1.
   Covered by the same shipped test cited for item 3.*
5. `same_branch_unresolved_gate_blocks_later_commit`
6. `matching_replay_clears_unresolved_gate`
7. `different_replay_preserves_unresolved_gate`
8. `durable_gate_serializes_or_tracks_multiple_unresolved_facts`
9. `durable_gate_close_requires_clean_state` —
   *Existing: `src/lifecycle/durable/close.rs:164-175` rejects close with
   `LifecycleError::CloseFailed` when `durable_gate.unresolved().is_some()`;
   `src/lifecycle/tests/durable.rs:675`
   `durable_close_does_not_report_complete_with_unresolved_durable_gate`
   verifies the contract.*
10. `durable_gate_error_codes_are_phase_specific`

Assertions:

1. every post-WAL failure has typed phase classification;
2. gate behavior is deterministic for same-branch and cross-branch cases.

#### Cache Mode (Phase 3 verification)

Cache-mode commits acquire the global durable admission lock at
`src/commit/cache.rs:77` and record `applied_not_visible` gate entries
with `CommitDurabilityClass::NotDurable` on visibility failure. The
following shipped tests verify the participation:

1. `cache_commit_rejects_any_unresolved_durable_gate_before_allocation` —
   *`src/commit/tests/cache.rs:935`. Verifies cache mode acquires the
   global admission lock and is blocked by a pre-recorded unresolved
   fact from another branch; asserts allocator not advanced, no rows
   applied, visible version unchanged, branch guard released.*
2. `cache_commit_visible_publication_failure_reports_applied_not_visible_and_releases_guard` —
   *`src/commit/tests/cache.rs:641`. Verifies cache-mode visibility
   failure records `CommitUnresolvedDurable` carrying
   `CommitDurabilityClass::NotDurable` (distinct from durable-mode
   `AppliedButNotVisible` which carries durable facts).*

Static enforcement: `mark_deleting_is_only_called_from_delete_branch`
in `crates/storage-next/tests/commit_runtime_source_guard.rs` scans
production source and rejects any `mark_deleting(` call outside
`branch_registry.rs` (definition) or
`branch_lifecycle::delete_branch` body.

### 8. Durability-Uncertain Outcomes

Required tests:

1. `always_policy_not_forced_returns_durability_uncertain`
2. `durability_uncertain_does_not_apply_rows_before_recovery`
3. `durability_uncertain_surviving_wal_record_replays`
4. `durability_uncertain_absent_wal_record_does_not_create_commit`
5. `durability_uncertain_allocator_catchup_is_monotonic`
6. `durability_uncertain_timestamp_catchup_is_monotonic`
7. `durability_uncertain_close_reports_residual_fact`
8. `durability_uncertain_error_source_identifies_wal_phase`
9. `durability_uncertain_generated_fault_uses_input_bytes`
10. `durability_uncertain_sensitivity_probe_is_recorded`

Assertions:

1. uncertain durability is neither success nor definite failure;
2. recovery handles both surviving and absent WAL records.

### 9. Timeline Hardening

Required tests:

1. `timeline_lookup_returns_greatest_version_at_or_before_timestamp` —
   *Existing: `src/commit/tests/timeline.rs` covers the greatest-version-
   at-or-before-timestamp lookup.*
2. `timeline_duplicate_timestamps_tiebreak_by_commit_version` —
   *Existing: `src/commit/timeline.rs::from_rows` sorts by
   `(timestamp, version)`; `timeline_keys_preserve_big_endian_order_and_branch_isolation`
   verifies the row-key shape that drives this.*
3. `timeline_key_shape_includes_branch_timestamp_and_version` —
   *Existing: `src/commit/tests/timeline.rs::timeline_keys_preserve_big_endian_order_and_branch_isolation`.*
4. `timeline_bounds_return_structured_earliest_latest_entries` —
   *Existing: `src/commit/tests/timeline.rs` covers bounds queries.*
5. `timeline_only_wal_payload_rejects` —
   *Shipped: `src/commit/replay.rs::validate_replay_rows` (lines
   312-341) rejects timeline-only payloads with
   `CommitRuntimeError::InvalidCommitState { reason: "replay payload
   is missing user mutation rows" }`. Test:
   `src/commit/tests/replay.rs::replay_rejects_timeline_only_payload_without_user_mutation`
   (line ~548).*
6. `timeline_user_rows_missing_rejects` —
   *Covered by item 5 (same `validate_replay_rows` path).*
7. `timeline_rows_missing_rejects` —
   *Existing: `src/commit/tests/replay.rs::replay_rejects_record_without_timeline_pair`.*
8. `timeline_branch_mismatch_rejects` —
   *Existing: `src/commit/timeline.rs::from_rows` filters by
   `branch_id` (line ~221); covered by
   `src/commit/tests/timeline.rs::timeline_version_lookup_is_branch_local`.*
9. `timeline_replay_duplicate_exact_is_idempotent` —
   *Existing: `src/commit/tests/replay.rs::replay_exact_duplicate_is_idempotent_catches_up_and_clears_matching_gate`.*
10. `timeline_replay_partial_pair_rejects` —
    *Existing: `src/commit/tests/replay.rs::replay_rejects_timeline_pair_that_disagrees_with_wal_facts`.*
11. `timeline_corruption_maps_to_typed_recovery_error` —
    *Existing: `src/commit/tests/replay.rs` covers typed-error paths
    for corruption.*
12. `branch_a_timeline_does_not_satisfy_branch_b_as_of` —
    *Existing: `src/commit/tests/timeline.rs::timeline_version_lookup_is_branch_local`
    verifies branch isolation in timeline lookups. Phase 6 also pins
    the fork-inheritance contract through three tests in
    `src/lifecycle/tests/branch_lifecycle/fork.rs`
    (`forked_branch_at_timestamp_before_fork_returns_parent_row`,
    `forked_branch_at_timestamp_after_fork_returns_child_row`,
    `forked_branch_isolated_from_parent_post_fork_commits`) that
    verify the corollary: a forked child uses inherited-layer reads
    (Option C per impl-plan §Open Questions §B), not a centralized
    parent-timeline lookup; parent post-fork commits stay invisible.*

Assertions:

1. timestamp lookup follows the storage timeline substrate;
2. replay cannot install timeline-only commits.

### 10. Outcome Validation

Required tests:

1. `visible_standard_outcome_requires_visible_fact`
2. `visible_standard_outcome_requires_matching_durable_fact_when_durable`
3. `not_durable_outcome_rejects_durable_fact`
4. `not_visible_outcome_rejects_visible_fact`
5. `durable_but_not_visible_preserves_durable_and_applied_facts`
6. `applied_but_not_visible_preserves_timeline_fact`
7. `invalid_outcome_reports_stable_code`
8. `outcome_display_is_not_test_oracle`
9. `outcome_source_chain_preserved_for_lower_layer_failure`
10. `outcome_validation_order_is_pinned`

Assertions:

1. impossible fact combinations cannot be constructed;
2. tests assert stable codes/classes.

### 11. Minimal Checkpoint And WAL-Growth Policy

Verifies shipped behavior in `crates/storage-next/src/lifecycle/wal_growth.rs`
and `crates/storage-next/src/lifecycle/tests/commit_hardening.rs` (lines 20-554);
not gating new implementation work. The 14 tests below already exist under
their listed names; the §11 assertions verify the existing matrix continues to
hold.

Required tests:

1. `automatic_checkpoint_triggers_when_wal_bytes_exceed_threshold`
2. `automatic_checkpoint_triggers_when_retained_segments_exceed_threshold`
3. `automatic_checkpoint_does_not_trigger_below_threshold`
4. `automatic_checkpoint_uses_existing_maintenance_executor`
5. `automatic_checkpoint_deferred_while_quiesce_active`
6. `automatic_checkpoint_deferred_while_close_in_progress`
7. `automatic_checkpoint_deferred_while_recovery_in_progress`
8. `automatic_checkpoint_failure_records_health_debt`
9. `automatic_checkpoint_does_not_truncate_wal_without_retention_proof`
10. `automatic_checkpoint_truncates_wal_only_after_checkpoint_or_table_manifest_proof`
11. `automatic_checkpoint_cache_mode_reports_no_durable_action`
12. `automatic_checkpoint_disable_requires_explicit_config`
13. `wal_growth_pressure_facts_have_stable_observation_api` —
    *Forward-looking name. The live test ships as
    `wal_growth_pressure_facts_are_visible_to_public_boundary`
    (`crates/storage-next/src/lifecycle/tests/commit_hardening.rs:375`); a
    future slice may rename the code to match this aspirational name. The
    "public boundary" wording in the live name predates the Pre-L9 rule
    requiring `pub(crate)` defaults.*
14. `automatic_checkpoint_policy_is_deterministic_without_background_thread`

Assertions:

1. V1 durable local mode cannot silently grow WAL past configured bounds;
2. threshold crossing produces either a checkpoint request, completed
   checkpoint, or typed deferred/health fact;
3. WAL truncation remains proof-gated;
4. the minimal policy does not introduce background nondeterminism.

Tests #5, #6, and #7 verify that policy evaluation defers when the lifecycle
state machine refuses admission for quiesce, close, or recovery. The deferral
comes from `LifecycleStateMachine::admit`, not from concurrent execution —
recovery is a lifecycle state, not a thread. The policy never runs concurrently
with quiesce, close, or recovery; it only checks the state machine before
enqueuing a checkpoint task.

## Generated Model Tests

Add a generated commit-hardening model with operations:

1. create branch descriptor;
2. commit blind write;
3. commit with read set;
4. commit with CAS;
5. hold branch guard;
6. begin/end quiesce;
7. inject WAL append/apply/visible failure;
8. replay WAL record;
9. publish timeline entry;
10. delete/recreate branch generation;
11. recover;
12. trigger automatic checkpoint/WAL-growth policy;
13. close.

Reference model requirements:

1. model stores commit versions, timestamps, branch generation, branch status,
   global visible version, hidden-applied rows, unresolved durable facts, and
   timeline entries;
2. model computes allowed admissions independently of production code;
3. model distinguishes durable-not-applied, applied-not-visible,
   not-durable, and durability-uncertain;
4. model tracks WAL growth threshold pressure and proof-gated truncation;
5. generated scripts must use input bytes to choose operations, branch ids,
   generations, timestamps, validation facts, and fault phases.

Required tests:

1. `commit_hardening_generated_model_matches_visible_facts`
2. `commit_hardening_generated_model_matches_generation_admission`
3. `commit_hardening_generated_model_matches_conflict_validation`
4. `commit_hardening_generated_model_matches_quiesce_admission`
5. `commit_hardening_generated_model_matches_durable_gate`
6. `commit_hardening_generated_model_matches_timeline_lookup`
7. `commit_hardening_generated_model_matches_replay`
8. `commit_hardening_generated_model_matches_checkpoint_trigger`
9. `commit_hardening_generated_model_no_hidden_visible_leak`

## Fault Windows

Plan-mode exploration (Phase 7) found that `tests/lifecycle_faults.rs`
already covers most of the listed scenarios at the lifecycle layer
(19 fault tests covering orphan snapshots, partial WAL, replay
failures, close-with-unresolved-gate, etc.). The commit-level
rejection tests in `commit/tests/{cache,durable,replay,durable_gate}.rs`
cover the remaining post-allocation / post-WAL / gate-mismatch
paths. Phase 7 annotates each item with its existing coverage
rather than re-implementing tests at the commit phase.

Required fault tests:

1. `fault_after_branch_guard_before_validation` —
   *Covered structurally by `CommitBranchGuard`'s RAII Drop; the
   guard releases on any error or panic returned before validation
   completes. See `commit/tests/guard.rs::branch_guard_serializes_same_branch_and_releases_on_drop`.*
2. `fault_after_validation_before_allocation` —
   *Covered by `cache_commit_rejects_branch_state_mismatch_before_allocation`
   (`commit/tests/cache.rs`) and `cache_commit_rejects_any_unresolved_durable_gate_before_allocation`
   (`commit/tests/cache.rs:935`); both fail at admission and assert
   allocator state unchanged.*
3. `fault_after_allocation_before_wal_append` —
   *Covered by `durable_uncertain_wal_failure_is_distinct_and_leaves_no_visible_rows`
   (`commit/tests/durable.rs:1065`) — allocator advances, WAL append
   fails, no row applied.*
4. `fault_after_wal_append_before_apply` —
   *Covered by `durable_apply_failure_after_wal_success_records_durable_not_applied_gate`
   (`commit/tests/durable.rs:1108`).*
5. `fault_after_apply_before_timeline_install` —
   *Covered by the `CacheCommitRows::prepare` validation path; the
   apply phase atomically writes user + timeline rows together
   (no fault window between them).*
6. `fault_after_timeline_install_before_visible_publish` —
   *Covered by `cache_commit_visible_publication_failure_reports_applied_not_visible_and_releases_guard`
   (`commit/tests/cache.rs:641`) and
   `durable_visibility_failure_after_apply_records_applied_not_visible_gate`
   (`commit/tests/durable.rs`).*
7. `fault_after_visible_publish_before_guard_release` —
   *Covered structurally by RAII drop on the admission guard and
   the durable-gate admission; existing tests assert guard release
   on success.*
8. `fault_during_unresolved_gate_record` —
   *Covered by `unresolved_durable_gate_records_idempotently_and_blocks_mutation`
   and `unresolved_durable_gate_rejects_different_fact_and_exact_clear`
   (`commit/tests/durable_gate.rs:241, 369`).*
9. `fault_during_quiesce_start` —
   *Covered by `quiesce_cannot_start_with_active_guards_or_while_already_active`
   (`commit/tests/guard.rs:119`).*
10. `fault_during_quiesce_release` —
    *Covered structurally by `CommitQuiesceGuard`'s RAII Drop;
    `quiesce_guard_released_on_retryable_failure_when_contract_allows_retry`
    (`lifecycle/tests/durable.rs:1294`) and
    `branch_lifecycle_quiesce_guard_releases_on_failure_so_followup_acquire_succeeds`
    (Phase 4) verify the release.*
11. `fault_during_replay_visible_publish` —
    *Existing: `tests/lifecycle_faults.rs:127`
    `fault_replay_visible_publication_failure_records_durable_not_visible`.*
12. `fault_during_automatic_checkpoint_request` —
    *Covered by `automatic_checkpoint_failure_records_health_debt`
    (`lifecycle/tests/commit_hardening.rs:218`).*
13. `fault_during_checkpoint_after_wal_growth_pressure` —
    *Covered by `automatic_checkpoint_does_not_truncate_wal_without_retention_proof`
    (`lifecycle/tests/commit_hardening.rs:528`) and the deferred
    paths verified by `automatic_checkpoint_deferred_while_quiesce_active`
    (line 262).*
14. `fault_during_wal_retention_after_checkpoint` —
    *Covered by `fault_manifest_updated_wal_truncation_fails_keeps_checkpoint_success`
    (`tests/lifecycle_faults.rs:77`).*
15. `fault_during_close_with_unresolved_gate` —
    *Covered by `durable_close_does_not_report_complete_with_unresolved_durable_gate`
    (`lifecycle/tests/durable.rs:675`) shipped under L8Z Phase 3.*

Audit-flagged additional edge cases:

- `fault_during_replay_gate_replace_exact` —
  *Covered by `unresolved_durable_gate_replaces_only_exact_existing_fact`
  (`commit/tests/durable_gate.rs:408`) which exercises the
  replace_exact failure paths (empty gate + different existing
  fact) that the replay flow at `commit/replay.rs:235` invokes.*
- `fault_during_conflict_validation_panic_safe` —
  *Covered structurally by Rust's panic-safety contracts on RAII
  guards (`CommitBranchGuard` and `CommitQuiesceGuard` Drop on
  panic). The `_admission_guard` and `_quiesce` RAII patterns in
  `commit/{cache,durable}.rs` and the Phase 4 branch-lifecycle
  wrappers release on unwinding.*
- `fault_after_allocation_partial_rollback` —
  *Covered by `cache_commit_apply_failure_releases_guard_without_visible_publication`
  (`commit/tests/cache.rs:589`) and
  `durable_apply_failure_after_wal_success_records_durable_not_applied_gate`
  (`commit/tests/durable.rs:1108`). Allocator advance after a
  post-allocation failure is intentional behavior; existing tests
  assert `last_allocated` reflects the advance.*
- `fault_during_replay_partial_wal_record` —
  *Covered by `fault_partial_wal_tail_strict_fails_before_repair`
  (`tests/lifecycle_faults.rs:87`) and
  `fault_partial_wal_tail_lossy_repairs_and_degrades_health`
  (`tests/lifecycle_faults.rs:97`).*

Assertions:

1. allocator, durability, applied, timeline, and visible facts remain coherent;
2. every guard is released or intentionally retained as a typed residual fact;
3. recovery can classify every fault window.

## Source Guards

Required source-guard tests:

1. commit/lifecycle code contains no V1 transaction-id allocator or WAL field;
2. commit/lifecycle code imports no engine, primitive, query, remote, hub, raw
   filesystem, environment, or network APIs;
3. commit code does not use product transaction wording in user-facing errors;
4. milestone labels do not appear in Rust code, test names, fixture bytes, or
   user-facing error strings (the latter caught via source scans of `format!`
   templates and `#[error(...)]` attributes);
5. L6 branch modules do not import commit hardening helpers;
6. L7 commit modules do not import lifecycle catalog types except through
   allowed guard/admission abstractions;
7. source guards scan production source, tests, and fuzz targets.

## Sensitivity Probes

Record each probe in the porting log with mutation site and fired test.

| Probe | Mutation | Expected failure |
|---|---|---|
| S1 | Add transaction id field to WAL commit payload. | Transaction-id source guard fails. |
| S2 | Skip generation guard on durable commit. | Stale generation durable test fails. |
| S3 | Skip generation guard on replay. | Replay stale generation test fails. |
| S4 | Validate read set after allocation. | Conflict-before-allocation test fails. |
| S5 | Validate CAS against latest instead of captured bound. | CAS captured-bound test fails. |
| S6 | Allow branch guard during quiesce. | Quiesce admission test fails. |
| S7 | Allow quiesce while branch guard active. | Quiesce start test fails. |
| S8 | Advance visible past hidden applied row. | Hidden visible leak test fails. |
| S9 | Return generic gate error for second cross-branch failure. | Cross-branch gate test fails. |
| S10 | Treat durability uncertain as success. | Durability-uncertain tests fail. |
| S11 | Accept timeline-only WAL payload. | Timeline-only replay test fails. |
| S12 | Use timestamp-only timeline key. | Duplicate timestamp tiebreak test fails. |
| S13 | Permit outcome with not-durable and durable fact. | Outcome validation test fails. |
| S14 | Drop source chain from lower-layer commit failure. | Source-chain test fails. |
| S15 | Add product transaction wording to error string. | Source guard fails. |
| S16 | Put milestone label in fixture bytes. | Source guard fails. |
| S17 | Disable checkpoint trigger after WAL threshold crossing. | WAL-growth policy test fails. |
| S18 | Truncate WAL without checkpoint/table-manifest proof. | Proof-gated truncation test fails. |

## Fuzz Targets

The audit recommended five `commit_hardening_*` fuzz targets. Phase 7
plan-mode exploration found that the existing 4 targets at
`crates/storage-next/tests/commit_runtime_fuzz_inventory.rs` already
cover the audit's hardening intent under different names. The
audit-faithful aliases are not adopted:

| Audit target | Existing target that covers it |
|---|---|
| `commit_hardening_admission` | `commit_runtime_batch` (`check_commit_runtime_batch_contract`) |
| `commit_hardening_durable_gate` | `commit_runtime_durable` (`check_commit_runtime_durable_contract`) |
| `commit_hardening_replay_timeline` | `commit_runtime_timeline` (`check_commit_runtime_timeline_contract`) |
| `commit_hardening_quiesce` | Structurally covered by Phase 4 quiesce wrapper tests (10+ tests in `lifecycle/tests/{durable,cache}.rs`) + the underlying mechanism in `commit/tests/guard.rs` (8 tests). A bespoke fuzz target would not expand the state space meaningfully. |
| `commit_hardening_checkpoint_policy` | Structurally covered by Phase 4 WAL-growth tests (16 tests in `lifecycle/tests/commit_hardening.rs`). The deterministic threshold model is exhaustive. |

Future slices may add hardening-specific fuzz targets if a failure
mode arises that the existing 4 don't cover. Pairwise distinctness
of the existing targets is verified by
`commit_runtime_closeout_fuzz_inventory_is_registered_seeded_and_distinct`
in `tests/commit_runtime_closeout.rs` and
`lifecycle_hardening_closeout_fuzz_targets_are_distinct` (Phase 7) in
`tests/lifecycle_closeout.rs`.

Rules (unchanged):

1. Each target decodes a distinct operation script.
2. Each target has at least two semantic seed corpus files (the
   existing pattern is `fault-script` + `generated-script`).
3. Closeout verifies pairwise contract distinctness, not just target
   names.
4. Generated counters distinguish input-derived coverage from
   canonical smoke coverage.

## Q-Z Closeout Tests

Required tests (Phase 7 dispositions):

1. `lifecycle_hardening_closeout_lists_q_to_z_plans` —
   *Phase 7 new: `tests/lifecycle_closeout.rs`. Verifies each
   L8Q-L8Z slice has implementation and test plan documents and
   that the porting log records each shipped phase.*
2. `lifecycle_hardening_closeout_source_guards_cover_q_to_z` —
   *Deferred. The existing
   `lifecycle_closeout_source_guards_cover_required_boundaries` at
   `tests/lifecycle_closeout.rs:278` already enumerates boundary
   categories (25 checks) including the L8Q-L8Z areas. A bespoke
   Q-Z wrapper would duplicate without adding coverage.*
3. `lifecycle_hardening_closeout_fuzz_targets_are_distinct` —
   *Phase 7 new: `tests/lifecycle_closeout.rs`. Wraps the existing
   per-area pairwise-distinctness checks under the Q-Z naming
   convention so a future inventory removal breaks the closeout.*
4. `lifecycle_hardening_closeout_seed_corpora_are_semantic` —
   *Deferred. Semantic content is hard to assert in CI; the
   structural property (seeded + pairwise distinct) is already
   covered by `lifecycle_closeout_fuzz_targets_and_corpora_are_distinct`
   at `tests/lifecycle_closeout.rs:208`.*
5. `lifecycle_hardening_closeout_sensitivity_ledger_has_mutation_rows` —
   *Phase 7 new: `tests/lifecycle_closeout.rs`. Asserts the
   porting log's L8Z section contains a sensitivity-probe ledger
   header and at least the expected row count.*
6. `lifecycle_hardening_closeout_command_matrix_records_pass_fail` —
   *Deferred. Covered indirectly by
   `lifecycle_hardening_closeout_sensitivity_ledger_has_mutation_rows`
   which validates the porting log's tables (sensitivity ledger
   + command matrix are co-located in the same L8Z section).*
7. `lifecycle_hardening_closeout_deferred_map_is_current` —
   *Deferred. Covered indirectly by
   `lifecycle_hardening_closeout_lists_q_to_z_plans` which
   validates planning-document presence + porting-log references
   (the deferred map lives in the porting log's closeout summary).*
8. `lifecycle_hardening_closeout_pre_l9_public_surface_is_crate_private` —
   *Phase 7 new: `tests/lifecycle_closeout.rs`. Asserts each
   surface (lifecycle, commit-runtime, branch-LSM, table-runtime)
   has its `*_stays_crate_private` source-guard test.*

These tests validate real code/test/source-guard inventory, not only
planning-document presence.

## Verification Commands

Run at slice closeout:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib commit
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --locked --test commit_runtime_source_guard
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test commit_runtime_faults
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_closeout
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_fuzz_inventory
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

If `cargo hack` is available:

```bash
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

No-default and wasm smoke:

```bash
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
```

Nightly fuzz smoke, when nightly and cargo-fuzz are available:

```bash
cargo +nightly fuzz run commit_hardening_admission -- -runs=256
cargo +nightly fuzz run commit_hardening_durable_gate -- -runs=256
cargo +nightly fuzz run commit_hardening_replay_timeline -- -runs=256
cargo +nightly fuzz run commit_hardening_quiesce -- -runs=256
cargo +nightly fuzz run commit_hardening_checkpoint_policy -- -runs=256
```

## Exit Gate

L8Z test coverage is complete when:

1. transaction-id absence or implementation is mechanically guarded;
2. generation guards cover every boundary-crossing branch operation;
3. conflict validation is tested before allocation/mutation;
4. quiesce is tested across checkpoint, branch lifecycle, recovery, close, and
   commit admission;
5. global visibility safety prevents hidden applied rows from becoming visible;
6. durable gate tests cover cross-branch post-WAL failures;
7. durability-uncertain outcomes are replay-tested;
8. timeline lookup and timeline-only WAL rejection are pinned;
9. outcome validation rejects impossible fact combinations;
10. automatic checkpoint/WAL-growth behavior is threshold-tested and
    proof-gated;
11. generated/fault/fuzz tests use input-derived scripts;
12. Q-Z closeout records source guards, probes, command outcomes, and
    deferrals.
