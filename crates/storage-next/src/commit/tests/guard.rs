use super::*;

#[test]
fn branch_guard_serializes_same_branch_and_releases_on_drop() {
    let branch = branch_id(110);
    let guard_set = CommitBranchGuardSet::new();

    let guard = guard_set
        .try_acquire_branch_guard(branch)
        .expect("acquire first guard");

    assert_eq!(guard.branch_id(), branch);
    assert_eq!(guard_set.active_guard_count(), Ok(1));
    assert_eq!(
        guard_set
            .try_acquire_branch_guard(branch)
            .expect_err("same branch contention rejects"),
        CommitRuntimeError::BranchGuardUnavailable {
            branch_id: branch,
            reason: "branch commit guard is already active",
        }
    );

    drop(guard);
    assert_eq!(guard_set.active_guard_count(), Ok(0));
    let reacquired = guard_set
        .try_acquire_branch_guard(branch)
        .expect("reacquire after drop");
    drop(reacquired);
    assert_eq!(guard_set.active_guard_count(), Ok(0));
}

#[test]
fn branch_guard_allows_different_branches_independently() {
    let branch_a = branch_id(111);
    let branch_b = branch_id(112);
    let guard_set = CommitBranchGuardSet::new();

    let guard_a = guard_set
        .try_acquire_branch_guard(branch_a)
        .expect("branch A guard");
    let guard_b = guard_set
        .try_acquire_branch_guard(branch_b)
        .expect("branch B guard");

    assert_eq!(guard_set.active_guard_count(), Ok(2));
    drop(guard_a);
    assert_eq!(guard_set.active_guard_count(), Ok(1));
    assert_eq!(
        guard_set
            .try_acquire_branch_guard(branch_b)
            .expect_err("branch B contention rejects"),
        CommitRuntimeError::BranchGuardUnavailable {
            branch_id: branch_b,
            reason: "branch commit guard is already active",
        }
    );
    drop(guard_b);
    assert_eq!(guard_set.active_guard_count(), Ok(0));
}

#[test]
fn cloned_guard_sets_share_branch_guard_state() {
    let branch = branch_id(117);
    let guard_set = CommitBranchGuardSet::new();
    let cloned = guard_set.clone();

    let guard = guard_set
        .try_acquire_branch_guard(branch)
        .expect("acquire through original");

    assert_eq!(cloned.active_guard_count(), Ok(1));
    assert_eq!(
        cloned
            .try_acquire_branch_guard(branch)
            .expect_err("clone observes contention"),
        CommitRuntimeError::BranchGuardUnavailable {
            branch_id: branch,
            reason: "branch commit guard is already active",
        }
    );

    drop(guard);
    assert_eq!(cloned.active_guard_count(), Ok(0));
    let reacquired = cloned
        .try_acquire_branch_guard(branch)
        .expect("reacquire through clone");
    assert_eq!(guard_set.active_guard_count(), Ok(1));
    drop(reacquired);
    assert_eq!(guard_set.active_guard_count(), Ok(0));
}

#[test]
fn quiesce_blocks_new_mutating_guards_until_token_drops() {
    let branch = branch_id(113);
    let guard_set = CommitBranchGuardSet::new();

    let quiesce = guard_set.try_begin_quiesce().expect("begin quiesce");

    assert_eq!(guard_set.is_quiescing(), Ok(true));
    assert_eq!(
        guard_set
            .try_acquire_branch_guard(branch)
            .expect_err("quiesce rejects mutating guard"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce is active",
        }
    );

    drop(quiesce);
    assert_eq!(guard_set.is_quiescing(), Ok(false));
    let guard = guard_set
        .try_acquire_branch_guard(branch)
        .expect("guard after quiesce");
    drop(guard);
}

#[test]
fn quiesce_cannot_start_with_active_guards_or_while_already_active() {
    let branch = branch_id(114);
    let guard_set = CommitBranchGuardSet::new();

    let guard = guard_set
        .try_acquire_branch_guard(branch)
        .expect("active guard");
    assert_eq!(
        guard_set
            .try_begin_quiesce()
            .expect_err("active guard rejects quiesce"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce cannot start while branch guards are active",
        }
    );
    assert_eq!(guard_set.is_quiescing(), Ok(false));
    drop(guard);

    let quiesce = guard_set.try_begin_quiesce().expect("begin quiesce");
    assert_eq!(
        guard_set
            .try_begin_quiesce()
            .expect_err("second quiesce rejects"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce is already active",
        }
    );
    drop(quiesce);
    assert_eq!(guard_set.is_quiescing(), Ok(false));
}

#[test]
fn quiesce_cannot_start_with_multiple_active_guards_and_failure_does_not_latch() {
    let branch_a = branch_id(118);
    let branch_b = branch_id(119);
    let guard_set = CommitBranchGuardSet::new();

    let guard_a = guard_set
        .try_acquire_branch_guard(branch_a)
        .expect("branch A guard");
    let guard_b = guard_set
        .try_acquire_branch_guard(branch_b)
        .expect("branch B guard");

    assert_eq!(guard_set.active_guard_count(), Ok(2));
    assert_eq!(
        guard_set
            .try_begin_quiesce()
            .expect_err("active guards reject quiesce"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce cannot start while branch guards are active",
        }
    );
    assert_eq!(guard_set.is_quiescing(), Ok(false));

    drop(guard_a);
    drop(guard_b);
    assert_eq!(guard_set.active_guard_count(), Ok(0));
    let later = guard_set
        .try_acquire_branch_guard(branch_a)
        .expect("failed quiesce did not block later guard");
    drop(later);
}

#[test]
fn cloned_guard_sets_share_quiesce_state() {
    let branch = branch_id(120);
    let guard_set = CommitBranchGuardSet::new();
    let cloned = guard_set.clone();

    let quiesce = cloned.try_begin_quiesce().expect("begin through clone");

    assert_eq!(guard_set.is_quiescing(), Ok(true));
    assert_eq!(
        guard_set
            .try_acquire_branch_guard(branch)
            .expect_err("original observes quiesce"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce is active",
        }
    );

    drop(quiesce);
    assert_eq!(guard_set.is_quiescing(), Ok(false));
    let guard = guard_set
        .try_acquire_branch_guard(branch)
        .expect("guard after clone quiesce drops");
    drop(guard);
}

#[test]
fn scripted_guard_interleaving_keeps_quiesce_and_branch_guards_exclusive() {
    let branch_a = branch_id(121);
    let branch_b = branch_id(122);
    let probe_branch = branch_id(123);
    let guard_set = CommitBranchGuardSet::new();

    let guard_a = guard_set
        .try_acquire_branch_guard(branch_a)
        .expect("branch A guard");
    let guard_b = guard_set
        .try_acquire_branch_guard(branch_b)
        .expect("branch B guard");
    assert_eq!(guard_set.active_guard_count(), Ok(2));
    assert_eq!(
        guard_set
            .try_begin_quiesce()
            .expect_err("active guards reject quiesce"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce cannot start while branch guards are active",
        }
    );

    drop(guard_a);
    assert_eq!(guard_set.active_guard_count(), Ok(1));
    assert_eq!(
        guard_set
            .try_begin_quiesce()
            .expect_err("remaining guard rejects quiesce"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce cannot start while branch guards are active",
        }
    );
    drop(guard_b);

    let quiesce = guard_set.try_begin_quiesce().expect("begin quiesce");
    assert_eq!(guard_set.is_quiescing(), Ok(true));
    assert_eq!(
        guard_set
            .try_acquire_branch_guard(branch_a)
            .expect_err("quiesce blocks branch A"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce is active",
        }
    );
    assert_eq!(
        guard_set
            .try_acquire_branch_guard(probe_branch)
            .expect_err("quiesce blocks independent branch"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce is active",
        }
    );
    drop(quiesce);

    let guard_a = guard_set
        .try_acquire_branch_guard(branch_a)
        .expect("branch A guard after quiesce");
    assert_eq!(
        guard_set
            .try_acquire_branch_guard(branch_a)
            .expect_err("same branch still serialized"),
        CommitRuntimeError::BranchGuardUnavailable {
            branch_id: branch_a,
            reason: "branch commit guard is already active",
        }
    );
    drop(guard_a);
    assert_eq!(guard_set.active_guard_count(), Ok(0));
    assert_eq!(guard_set.is_quiescing(), Ok(false));
}

#[test]
fn read_only_diagnostic_does_not_use_mutating_guard_during_quiesce() {
    let branch = branch_id(115);
    let guard_set = CommitBranchGuardSet::new();
    let visible = VisibleVersionTracker::new(CommitVersion::new(17));
    let batch = read_only_batch(branch, CommitBatchOptions::default());
    let quiesce = guard_set.try_begin_quiesce().expect("begin quiesce");

    let outcome = execute_read_only_diagnostic(&batch, &CommitRuntimeConfig::default(), visible)
        .expect("read-only during quiesce");

    assert_eq!(outcome.kind(), CommitOutcomeKind::ReadOnly);
    assert_eq!(outcome.branch_id(), branch);
    assert_eq!(guard_set.active_guard_count(), Ok(0));
    assert_eq!(guard_set.is_quiescing(), Ok(true));
    drop(quiesce);
}

#[test]
fn guard_debug_output_is_bounded() {
    let branch = branch_id(116);
    let guard_set = CommitBranchGuardSet::new();
    let guard = guard_set
        .try_acquire_branch_guard(branch)
        .expect("branch guard");
    let set_debug = format!("{guard_set:?}");
    let guard_debug = format!("{guard:?}");

    assert_bounded_storage_debug(&set_debug);
    assert_bounded_storage_debug(&guard_debug);
    assert!(set_debug.contains("active_guard_count"));
    drop(guard);

    let quiesce = guard_set.try_begin_quiesce().expect("quiesce");
    assert_bounded_storage_debug(&format!("{quiesce:?}"));
    drop(quiesce);
}
