use super::*;

#[test]
fn branch_generation_rejects_zero_and_preserves_nonzero_fact() {
    assert_eq!(
        CommitBranchGeneration::new(0),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "branch generation must be nonzero",
        })
    );

    let generation = CommitBranchGeneration::new(42).expect("generation");
    assert_eq!(generation.get(), 42);
    assert_eq!(
        CommitBranchGenerationGuard::exact(generation),
        CommitBranchGenerationGuard::Exact(generation)
    );
    assert_eq!(
        CommitBranchGenerationGuard::not_supplied(),
        CommitBranchGenerationGuard::NotSupplied
    );
}

#[test]
fn branch_registry_registers_and_looks_up_storage_descriptors() {
    let branch = branch_id(91);
    let generation = generation(1);
    let mut registry = CommitBranchRegistry::new();

    let descriptor = registry
        .register_active(branch, generation)
        .expect("register branch");

    assert_eq!(descriptor.branch_id(), branch);
    assert_eq!(descriptor.generation(), generation);
    assert_eq!(descriptor.state(), CommitBranchState::Active);
    assert_eq!(registry.lookup(branch), Ok(descriptor));
    assert_eq!(registry.len(), 1);
}

#[test]
fn branch_registry_keeps_multiple_branch_descriptors_isolated() {
    let branch_a = branch_id(90);
    let branch_b = branch_id(91);
    let mut registry = CommitBranchRegistry::new();

    let descriptor_a = registry
        .register_active(branch_a, generation(1))
        .expect("register branch A");
    let descriptor_b = registry
        .register_active(branch_b, generation(2))
        .expect("register branch B");

    registry.mark_deleting(branch_a).expect("mark A deleting");
    assert_eq!(
        registry.lookup(branch_a).expect("branch A").state(),
        CommitBranchState::Deleting
    );
    assert_eq!(registry.lookup(branch_b), Ok(descriptor_b));

    registry.mark_deleted(branch_a).expect("mark A deleted");
    assert_eq!(
        registry.lookup(branch_a).expect("branch A").state(),
        CommitBranchState::Deleted
    );
    assert_eq!(registry.lookup(branch_b), Ok(descriptor_b));
    assert_ne!(descriptor_a.branch_id(), descriptor_b.branch_id());
    assert_eq!(registry.len(), 2);
}

#[test]
fn branch_registry_rejects_duplicate_and_mismatched_descriptors() {
    let branch = branch_id(92);
    let other = branch_id(93);
    let mut registry = CommitBranchRegistry::new();

    registry
        .register_active(branch, generation(1))
        .expect("register branch");

    assert_eq!(
        registry.register_active(branch, generation(2)),
        Err(CommitRuntimeError::BranchAlreadyExists { branch_id: branch })
    );
    assert_eq!(
        registry.register_descriptor(branch, CommitBranchDescriptor::active(other, generation(1)),),
        Err(CommitRuntimeError::BranchMismatch {
            expected: branch,
            actual: other,
        })
    );
}

#[test]
fn branch_registry_accepts_explicit_nonactive_descriptors() {
    let deleting = branch_id(89);
    let deleted = branch_id(88);
    let mut registry = CommitBranchRegistry::new();

    let deleting_descriptor = registry
        .register_descriptor(
            deleting,
            CommitBranchDescriptor::new(deleting, generation(4), CommitBranchState::Deleting),
        )
        .expect("register deleting descriptor");
    let deleted_descriptor = registry
        .register_descriptor(
            deleted,
            CommitBranchDescriptor::new(deleted, generation(5), CommitBranchState::Deleted),
        )
        .expect("register deleted descriptor");

    assert_eq!(registry.lookup(deleting), Ok(deleting_descriptor));
    assert_eq!(registry.lookup(deleted), Ok(deleted_descriptor));
    assert_eq!(
        registry.validate_target(deleting, CommitBranchGenerationGuard::not_supplied()),
        Err(CommitRuntimeError::BranchNotWritable {
            branch_id: deleting,
            reason: "branch is deleting",
        })
    );
    assert_eq!(
        registry.validate_target(deleted, CommitBranchGenerationGuard::not_supplied()),
        Err(CommitRuntimeError::BranchNotWritable {
            branch_id: deleted,
            reason: "branch is deleted",
        })
    );
}

#[test]
fn branch_registry_rejects_missing_deleting_and_deleted_targets() {
    let branch = branch_id(94);
    let missing = branch_id(95);
    let mut registry = CommitBranchRegistry::new();

    assert_eq!(
        registry.lookup(missing),
        Err(CommitRuntimeError::BranchNotFound { branch_id: missing })
    );
    assert_eq!(
        registry.validate_target(missing, CommitBranchGenerationGuard::not_supplied()),
        Err(CommitRuntimeError::BranchNotFound { branch_id: missing })
    );

    registry
        .register_active(branch, generation(1))
        .expect("register branch");
    registry.mark_deleting(branch).expect("mark deleting");
    assert_eq!(
        registry.validate_target(branch, CommitBranchGenerationGuard::not_supplied()),
        Err(CommitRuntimeError::BranchNotWritable {
            branch_id: branch,
            reason: "branch is deleting",
        })
    );

    registry.mark_deleted(branch).expect("mark deleted");
    assert_eq!(
        registry.validate_target(branch, CommitBranchGenerationGuard::not_supplied()),
        Err(CommitRuntimeError::BranchNotWritable {
            branch_id: branch,
            reason: "branch is deleted",
        })
    );
}

#[test]
fn branch_registry_validates_optional_and_exact_generation_facts() {
    let branch = branch_id(96);
    let current = generation(7);
    let stale = generation(6);
    let mut registry = CommitBranchRegistry::new();
    registry
        .register_active(branch, current)
        .expect("register branch");

    let without_generation = registry
        .validate_target(branch, CommitBranchGenerationGuard::not_supplied())
        .expect("optional generation accepted");
    let exact_generation = registry
        .validate_target(branch, CommitBranchGenerationGuard::exact(current))
        .expect("exact generation accepted");

    assert_eq!(without_generation.branch_id(), branch);
    assert_eq!(without_generation.generation(), current);
    assert_eq!(without_generation.state(), CommitBranchState::Active);
    assert_eq!(exact_generation, without_generation);
    assert_eq!(
        registry.validate_target(branch, CommitBranchGenerationGuard::exact(stale)),
        Err(CommitRuntimeError::BranchGenerationMismatch {
            branch_id: branch,
            expected: current.get(),
            actual: stale.get(),
        })
    );
}

#[test]
fn branch_registry_recreate_rejects_stale_generation_after_delete() {
    let branch = branch_id(97);
    let mut registry = CommitBranchRegistry::new();
    registry
        .register_active(branch, generation(3))
        .expect("register branch");
    registry.mark_deleted(branch).expect("mark deleted");

    assert_eq!(
        registry.recreate_active(branch, generation(3)),
        Err(CommitRuntimeError::BranchGenerationMismatch {
            branch_id: branch,
            expected: 4,
            actual: 3,
        })
    );

    let recreated = registry
        .recreate_active(branch, generation(4))
        .expect("recreate branch");
    assert_eq!(recreated.state(), CommitBranchState::Active);
    assert_eq!(
        registry.validate_target(branch, CommitBranchGenerationGuard::exact(generation(3))),
        Err(CommitRuntimeError::BranchGenerationMismatch {
            branch_id: branch,
            expected: 4,
            actual: 3,
        })
    );
}

#[test]
fn branch_registry_recreate_requires_deleted_state_and_newer_generation() {
    let active = branch_id(87);
    let deleting = branch_id(86);
    let deleted = branch_id(85);
    let mut registry = CommitBranchRegistry::new();

    registry
        .register_active(active, generation(1))
        .expect("register active branch");
    registry
        .register_active(deleting, generation(2))
        .expect("register deleting branch");
    registry
        .register_active(deleted, generation(3))
        .expect("register deleted branch");
    registry.mark_deleting(deleting).expect("mark deleting");
    registry.mark_deleted(deleted).expect("mark deleted");

    assert_eq!(
        registry.recreate_active(active, generation(2)),
        Err(CommitRuntimeError::BranchAlreadyExists { branch_id: active })
    );
    assert_eq!(
        registry.recreate_active(deleting, generation(3)),
        Err(CommitRuntimeError::BranchAlreadyExists {
            branch_id: deleting,
        })
    );

    let recreated = registry
        .recreate_active(deleted, generation(9))
        .expect("recreate with newer generation");
    assert_eq!(recreated.branch_id(), deleted);
    assert_eq!(recreated.generation(), generation(9));
    assert_eq!(recreated.state(), CommitBranchState::Active);
}

#[test]
fn branch_registry_lifecycle_markers_reject_missing_branches() {
    let branch = branch_id(103);
    let mut registry = CommitBranchRegistry::new();

    assert_eq!(
        registry.mark_deleting(branch),
        Err(CommitRuntimeError::BranchNotFound { branch_id: branch })
    );
    assert_eq!(
        registry.mark_deleted(branch),
        Err(CommitRuntimeError::BranchNotFound { branch_id: branch })
    );
}

#[test]
fn branch_registry_recreate_reports_exhausted_generation_at_max() {
    let branch = branch_id(104);
    let mut registry = CommitBranchRegistry::new();
    registry
        .register_active(branch, generation(u64::MAX))
        .expect("register max generation");
    registry.mark_deleted(branch).expect("mark deleted");

    assert_eq!(
        registry.recreate_active(branch, generation(u64::MAX)),
        Err(CommitRuntimeError::BranchGenerationExhausted {
            branch_id: branch,
            generation: u64::MAX,
        })
    );
}

#[test]
fn mutating_admission_returns_branch_facts_and_guard_token() {
    let branch = branch_id(98);
    let mut registry = CommitBranchRegistry::new();
    let guard_set = CommitBranchGuardSet::new();
    registry
        .register_active(branch, generation(9))
        .expect("register branch");
    let batch = mutating_delete_batch(branch, 0x20, b"delete".to_vec());

    let admitted = super::super::admit_mutating_commit(
        &registry,
        &guard_set,
        &batch,
        CommitBranchGenerationGuard::exact(generation(9)),
    )
    .expect("admit mutating commit");

    let admission = admitted.admission();
    assert_eq!(admission.branch_id(), branch);
    assert_eq!(admission.generation(), generation(9));
    assert_eq!(admission.state(), CommitBranchState::Active);
    assert_eq!(guard_set.active_guard_count(), Ok(1));
    drop(admitted);
    assert_eq!(guard_set.active_guard_count(), Ok(0));
}

#[test]
fn mutating_admission_accepts_unsupplied_generation_for_active_branch() {
    let branch = branch_id(84);
    let mut registry = CommitBranchRegistry::new();
    let guard_set = CommitBranchGuardSet::new();
    registry
        .register_active(branch, generation(11))
        .expect("register branch");
    let batch = mutating_delete_batch(branch, 0x20, b"delete".to_vec());

    let admitted = super::super::admit_mutating_commit(
        &registry,
        &guard_set,
        &batch,
        CommitBranchGenerationGuard::not_supplied(),
    )
    .expect("admit mutating commit without generation");

    assert_eq!(admitted.admission().branch_id(), branch);
    assert_eq!(admitted.admission().generation(), generation(11));
    assert_eq!(admitted.admission().state(), CommitBranchState::Active);
    assert_eq!(guard_set.active_guard_count(), Ok(1));
}

#[test]
fn mutating_admission_rejects_before_guard_for_bad_targets() {
    let branch = branch_id(99);
    let missing = branch_id(100);
    let mut registry = CommitBranchRegistry::new();
    let guard_set = CommitBranchGuardSet::new();
    registry
        .register_active(branch, generation(1))
        .expect("register branch");
    let missing_batch = mutating_delete_batch(missing, 0x20, b"missing".to_vec());
    let stale_batch = mutating_delete_batch(branch, 0x20, b"stale".to_vec());

    assert_eq!(
        super::super::admit_mutating_commit(
            &registry,
            &guard_set,
            &missing_batch,
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect_err("missing branch rejects"),
        CommitRuntimeError::BranchNotFound { branch_id: missing }
    );
    assert_eq!(
        super::super::admit_mutating_commit(
            &registry,
            &guard_set,
            &stale_batch,
            CommitBranchGenerationGuard::exact(generation(2)),
        )
        .expect_err("stale generation rejects"),
        CommitRuntimeError::BranchGenerationMismatch {
            branch_id: branch,
            expected: 1,
            actual: 2,
        }
    );
    assert_eq!(guard_set.active_guard_count(), Ok(0));
}

#[test]
fn mutating_admission_rejects_deleting_and_deleted_before_guard() {
    let branch = branch_id(83);
    let mut registry = CommitBranchRegistry::new();
    let guard_set = CommitBranchGuardSet::new();
    registry
        .register_active(branch, generation(1))
        .expect("register branch");
    let batch = mutating_delete_batch(branch, 0x20, b"blocked".to_vec());

    registry.mark_deleting(branch).expect("mark deleting");
    assert_eq!(
        super::super::admit_mutating_commit(
            &registry,
            &guard_set,
            &batch,
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect_err("deleting branch rejects"),
        CommitRuntimeError::BranchNotWritable {
            branch_id: branch,
            reason: "branch is deleting",
        }
    );
    assert_eq!(guard_set.active_guard_count(), Ok(0));

    registry.mark_deleted(branch).expect("mark deleted");
    assert_eq!(
        super::super::admit_mutating_commit(
            &registry,
            &guard_set,
            &batch,
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect_err("deleted branch rejects"),
        CommitRuntimeError::BranchNotWritable {
            branch_id: branch,
            reason: "branch is deleted",
        }
    );
    assert_eq!(guard_set.active_guard_count(), Ok(0));
}

#[test]
fn mutating_admission_serializes_same_branch_and_keeps_other_branch_available() {
    let branch_a = branch_id(82);
    let branch_b = branch_id(81);
    let mut registry = CommitBranchRegistry::new();
    let guard_set = CommitBranchGuardSet::new();
    registry
        .register_active(branch_a, generation(1))
        .expect("register branch A");
    registry
        .register_active(branch_b, generation(1))
        .expect("register branch B");
    let batch_a = mutating_delete_batch(branch_a, 0x20, b"a".to_vec());
    let batch_b = mutating_delete_batch(branch_b, 0x20, b"b".to_vec());

    let admitted_a = super::super::admit_mutating_commit(
        &registry,
        &guard_set,
        &batch_a,
        CommitBranchGenerationGuard::not_supplied(),
    )
    .expect("admit branch A");
    assert_eq!(
        super::super::admit_mutating_commit(
            &registry,
            &guard_set,
            &batch_a,
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect_err("same branch rejects"),
        CommitRuntimeError::BranchGuardUnavailable {
            branch_id: branch_a,
            reason: "branch commit guard is already active",
        }
    );

    let admitted_b = super::super::admit_mutating_commit(
        &registry,
        &guard_set,
        &batch_b,
        CommitBranchGenerationGuard::not_supplied(),
    )
    .expect("admit branch B while branch A active");
    assert_eq!(guard_set.active_guard_count(), Ok(2));

    drop(admitted_a);
    assert_eq!(guard_set.active_guard_count(), Ok(1));
    drop(admitted_b);
    assert_eq!(guard_set.active_guard_count(), Ok(0));
}

#[test]
fn mutating_admission_rejects_during_quiesce_before_branch_guard() {
    let branch = branch_id(80);
    let mut registry = CommitBranchRegistry::new();
    let guard_set = CommitBranchGuardSet::new();
    registry
        .register_active(branch, generation(1))
        .expect("register branch");
    let batch = mutating_delete_batch(branch, 0x20, b"quiesce".to_vec());
    let quiesce = guard_set.try_begin_quiesce().expect("begin quiesce");

    assert_eq!(
        super::super::admit_mutating_commit(
            &registry,
            &guard_set,
            &batch,
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect_err("quiesce rejects admission"),
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce is active",
        }
    );
    assert_eq!(guard_set.active_guard_count(), Ok(0));
    drop(quiesce);
}

#[test]
fn mutating_admission_rejects_read_only_batch_without_guard() {
    let branch = branch_id(101);
    let mut registry = CommitBranchRegistry::new();
    let guard_set = CommitBranchGuardSet::new();
    registry
        .register_active(branch, generation(1))
        .expect("register branch");
    let batch = read_only_batch(branch, CommitBatchOptions::default());

    assert_eq!(
        super::super::admit_mutating_commit(
            &registry,
            &guard_set,
            &batch,
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect_err("read-only batch rejects"),
        CommitRuntimeError::InvalidBatch {
            reason: "mutating branch admission requires mutating batch",
        }
    );
    assert_eq!(guard_set.active_guard_count(), Ok(0));
}

#[test]
fn branch_registry_errors_use_storage_vocabulary() {
    let branch = branch_id(102);
    let errors = [
        CommitRuntimeError::BranchAlreadyExists { branch_id: branch },
        CommitRuntimeError::BranchNotFound { branch_id: branch },
        CommitRuntimeError::BranchNotWritable {
            branch_id: branch,
            reason: "branch is deleting",
        },
        CommitRuntimeError::BranchGenerationMismatch {
            branch_id: branch,
            expected: 2,
            actual: 1,
        },
        CommitRuntimeError::BranchGenerationExhausted {
            branch_id: branch,
            generation: u64::MAX,
        },
        CommitRuntimeError::BranchGuardUnavailable {
            branch_id: branch,
            reason: "branch commit guard is already active",
        },
        CommitRuntimeError::CommitQuiesceUnavailable {
            reason: "commit quiesce is active",
        },
    ];

    for error in errors {
        let display = error.to_string();
        assert_bounded_storage_debug(&display);
    }
}

fn generation(value: u64) -> CommitBranchGeneration {
    CommitBranchGeneration::new(value).expect("generation")
}

fn mutating_delete_batch(
    branch: BranchId,
    storage_space: u8,
    bytes: Vec<u8>,
) -> ValidatedCommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::delete(physical_key(
            branch,
            storage_space,
            bytes,
        ))],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .expect("valid mutating batch")
}
