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

#[cfg(feature = "perf-trace")]
#[test]
fn guard_perf_trace_counts_branch_and_quiesce_contention() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(121);
    let guard_set = CommitBranchGuardSet::new();

    let guard = guard_set
        .try_acquire_branch_guard(branch)
        .expect("acquire first guard");
    assert_eq!(
        guard_set
            .try_acquire_branch_guard(branch)
            .expect_err("same branch contention rejects"),
        CommitRuntimeError::BranchGuardUnavailable {
            branch_id: branch,
            reason: "branch commit guard is already active",
        }
    );
    assert_eq!(
        guard_set
            .try_begin_quiesce()
            .expect_err("active branch guard rejects quiesce"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce cannot start while branch guards are active",
        }
    );
    drop(guard);

    let quiesce = guard_set.try_begin_quiesce().expect("begin quiesce");
    assert_eq!(
        guard_set
            .try_acquire_branch_guard(branch)
            .expect_err("quiesce rejects branch guard"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce is active",
        }
    );
    assert_eq!(
        guard_set
            .try_begin_quiesce()
            .expect_err("second quiesce rejects"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce is already active",
        }
    );

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.commit_branch_guard_attempts(), 3);
    assert_eq!(perf.commit_branch_guard_acquired(), 1);
    assert_eq!(perf.commit_branch_guard_rejected(), 2);
    assert_eq!(perf.commit_quiesce_attempts(), 3);
    assert_eq!(perf.commit_quiesce_acquired(), 1);
    assert_eq!(perf.commit_quiesce_rejected(), 2);
    drop(quiesce);
}

#[test]
fn lock_order_documentation_covers_commit_admission_sites() {
    let guard_source = include_str!("../guard.rs");
    let branch_registry_source = include_str!("../branch_registry.rs");
    let cache_source = include_str!("../cache.rs");
    let durable_source = include_str!("../durable.rs");
    let durable_gate_source = include_str!("../durable_gate.rs");

    assert!(guard_source.contains("Commit-runtime lock order"));
    assert!(guard_source.contains("unresolved-durable gate"));
    assert!(guard_source.contains("logical admission tokens"));
    assert!(guard_source.contains("no gate"));
    assert!(guard_source.contains("Branch registry validation"));
    assert!(guard_source.contains("try_acquire_branch_guard"));
    assert!(guard_source.contains("try_begin_quiesce"));
    assert!(guard_source.contains("BranchGuardUnavailable"));
    assert!(guard_source.contains("CommitQuiesceUnavailable"));

    assert!(branch_registry_source.contains("Lock order: registry validation is lock-free"));
    assert!(cache_source.contains("Commit-runtime lock order"));
    assert!(cache_source.contains("global admission token remains active"));
    assert!(cache_source.contains("does not carry a mutex guard"));
    assert!(cache_source.contains("retry/deadline policy belongs above"));
    assert!(durable_source.contains("Commit-runtime lock order"));
    assert!(durable_source.contains("global admission token remains active"));
    assert!(durable_source.contains("does not carry a mutex guard"));
    assert!(durable_source.contains("typed, fail-fast retry condition"));
    assert!(durable_gate_source.contains("first mutating-commit admission point"));
    assert!(durable_gate_source.contains("after the gate mutex has been released"));
}

#[test]
fn commit_runtime_sources_do_not_start_maintenance_work() {
    let production_sources = [
        ("allocator.rs", include_str!("../allocator.rs")),
        ("batch.rs", include_str!("../batch.rs")),
        ("branch_registry.rs", include_str!("../branch_registry.rs")),
        ("cache.rs", include_str!("../cache.rs")),
        ("config.rs", include_str!("../config.rs")),
        ("conflict.rs", include_str!("../conflict.rs")),
        ("durable.rs", include_str!("../durable.rs")),
        ("durable_gate.rs", include_str!("../durable_gate.rs")),
        ("error.rs", include_str!("../error.rs")),
        ("facts.rs", include_str!("../facts.rs")),
        ("guard.rs", include_str!("../guard.rs")),
        ("outcome.rs", include_str!("../outcome.rs")),
        ("replay.rs", include_str!("../replay.rs")),
        ("timeline.rs", include_str!("../timeline.rs")),
        ("visibility.rs", include_str!("../visibility.rs")),
    ];
    let forbidden_action_references = [
        "MaintenanceTask",
        "compact_cache_branch",
        "compact_branch",
        "flush_cache_branch",
        "flush_branch",
        "schedule_compaction",
        "schedule_flush",
        "schedule_maintenance",
        "materialization",
        "rate_limiter",
        "thread::sleep",
        "tokio::time::sleep",
    ];
    let roadmap_labels = [["M", "4", "P"].concat(), ["L", "7"].concat()];

    for (name, source) in production_sources {
        for label in &roadmap_labels {
            assert!(
                !source.contains(label),
                "{name} contains roadmap label text"
            );
        }
        for forbidden in forbidden_action_references {
            assert!(
                !source.contains(forbidden),
                "{name} unexpectedly references {forbidden}"
            );
        }
    }
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
