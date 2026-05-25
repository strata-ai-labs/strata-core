use super::*;

#[test]
fn lifecycle_state_machine_initial_state_admits_only_open_and_health() {
    let machine = LifecycleStateMachine::new();

    assert_eq!(machine.state(), LifecycleState::New);
    assert_eq!(machine.failure(), None);
    assert_eq!(machine.close_fact(), None);
    assert_allowed(
        machine.admit(LifecycleOperationKind::Open),
        LifecycleAdmissionEffect::Ordinary,
    );
    assert_allowed(
        machine.admit(LifecycleOperationKind::HealthQuery),
        LifecycleAdmissionEffect::Ordinary,
    );
    for operation in [
        LifecycleOperationKind::OrdinaryRead,
        LifecycleOperationKind::Commit,
        LifecycleOperationKind::RecoveryStep,
        LifecycleOperationKind::OrdinaryMaintenance,
        LifecycleOperationKind::CloseRequiredDrain,
        LifecycleOperationKind::Close,
        LifecycleOperationKind::CloseRetry,
    ] {
        assert_rejected(machine.admit(operation));
    }
}

#[test]
fn lifecycle_state_machine_accepts_open_and_recovery_transitions() {
    for case in [
        (
            LifecycleState::New,
            LifecycleTransitionTrigger::OpenRequested,
            LifecycleState::Opening,
            None,
            None,
            false,
        ),
        (
            LifecycleState::Opening,
            LifecycleTransitionTrigger::CacheOpenReady,
            LifecycleState::Open,
            None,
            None,
            false,
        ),
        (
            LifecycleState::Opening,
            LifecycleTransitionTrigger::DurableRecoveryRequired,
            LifecycleState::Recovering,
            None,
            None,
            false,
        ),
        (
            LifecycleState::Opening,
            LifecycleTransitionTrigger::PhaseFailed {
                reason: "open failed",
            },
            LifecycleState::Failed,
            None,
            Some(LifecycleState::Opening),
            false,
        ),
        (
            LifecycleState::Recovering,
            LifecycleTransitionTrigger::RecoveryAccepted,
            LifecycleState::Open,
            None,
            None,
            false,
        ),
        (
            LifecycleState::Recovering,
            LifecycleTransitionTrigger::PhaseFailed {
                reason: "recovery failed",
            },
            LifecycleState::Failed,
            None,
            Some(LifecycleState::Recovering),
            false,
        ),
    ] {
        assert_valid_transition(case);
    }
}

#[test]
fn lifecycle_state_machine_accepts_close_and_retry_transitions() {
    for case in [
        (
            LifecycleState::Open,
            LifecycleTransitionTrigger::CloseRequested,
            LifecycleState::Closing,
            Some(LifecycleCloseFact::Requested),
            None,
            false,
        ),
        (
            LifecycleState::Closing,
            LifecycleTransitionTrigger::CloseCompleted,
            LifecycleState::Closed,
            Some(LifecycleCloseFact::Complete),
            None,
            false,
        ),
        (
            LifecycleState::Closing,
            LifecycleTransitionTrigger::CloseRetried,
            LifecycleState::Closing,
            Some(LifecycleCloseFact::RetryPending),
            None,
            false,
        ),
        (
            LifecycleState::Closing,
            LifecycleTransitionTrigger::PhaseFailed {
                reason: "close failed",
            },
            LifecycleState::Failed,
            None,
            Some(LifecycleState::Closing),
            false,
        ),
        (
            LifecycleState::Closed,
            LifecycleTransitionTrigger::CloseRetried,
            LifecycleState::Closed,
            Some(LifecycleCloseFact::AlreadyClosed),
            None,
            true,
        ),
    ] {
        assert_valid_transition(case);
    }
}

#[test]
fn lifecycle_state_machine_rejects_undocumented_transitions_without_mutating_state() {
    for (start, trigger) in [
        (
            LifecycleState::New,
            LifecycleTransitionTrigger::CacheOpenReady,
        ),
        (
            LifecycleState::New,
            LifecycleTransitionTrigger::RecoveryAccepted,
        ),
        (
            LifecycleState::Opening,
            LifecycleTransitionTrigger::CloseCompleted,
        ),
        (
            LifecycleState::Recovering,
            LifecycleTransitionTrigger::CloseRequested,
        ),
        (
            LifecycleState::Open,
            LifecycleTransitionTrigger::DurableRecoveryRequired,
        ),
        (
            LifecycleState::Closed,
            LifecycleTransitionTrigger::OpenRequested,
        ),
        (
            LifecycleState::Closed,
            LifecycleTransitionTrigger::RecoveryAccepted,
        ),
        (
            LifecycleState::Failed,
            LifecycleTransitionTrigger::OpenRequested,
        ),
        (
            LifecycleState::Failed,
            LifecycleTransitionTrigger::CloseCompleted,
        ),
        (
            LifecycleState::Failed,
            LifecycleTransitionTrigger::CloseRetried,
        ),
        (
            LifecycleState::New,
            LifecycleTransitionTrigger::PhaseFailed { reason: "no phase" },
        ),
        (
            LifecycleState::Open,
            LifecycleTransitionTrigger::PhaseFailed {
                reason: "open ordinary phase",
            },
        ),
        (
            LifecycleState::Closed,
            LifecycleTransitionTrigger::PhaseFailed {
                reason: "already closed",
            },
        ),
        (
            LifecycleState::Failed,
            LifecycleTransitionTrigger::PhaseFailed {
                reason: "already failed",
            },
        ),
    ] {
        let mut machine = machine_in_state(start);
        let failure_before = machine.failure();
        let close_before = machine.close_fact();

        let error = machine
            .transition(trigger)
            .expect_err("invalid transition rejects");
        assert_eq!(
            error,
            LifecycleError::InvalidLifecycleState {
                reason: "invalid lifecycle transition",
            }
        );
        assert_bounded_storage_display(&error.to_string());
        assert_eq!(machine.state(), start);
        assert_eq!(machine.failure(), failure_before);
        assert_eq!(machine.close_fact(), close_before);
    }
}

#[test]
fn lifecycle_operation_admission_matrix_is_state_specific() {
    for state in [
        LifecycleState::New,
        LifecycleState::Opening,
        LifecycleState::Recovering,
        LifecycleState::Open,
        LifecycleState::Closing,
        LifecycleState::Closed,
        LifecycleState::Failed,
    ] {
        for operation in [
            LifecycleOperationKind::Open,
            LifecycleOperationKind::OrdinaryRead,
            LifecycleOperationKind::Commit,
            LifecycleOperationKind::RecoveryStep,
            LifecycleOperationKind::OrdinaryMaintenance,
            LifecycleOperationKind::CloseRequiredDrain,
            LifecycleOperationKind::HealthQuery,
            LifecycleOperationKind::Close,
            LifecycleOperationKind::CloseRetry,
        ] {
            assert_eq!(
                LifecycleStateMachine::admit_state(state, operation),
                expected_admission(state, operation),
                "unexpected admission for {state:?} {operation:?}"
            );
        }
    }
}

#[test]
fn lifecycle_failure_facts_preserve_phase_and_reject_empty_reasons() {
    for state in [
        LifecycleState::Opening,
        LifecycleState::Recovering,
        LifecycleState::Closing,
    ] {
        let mut machine = machine_in_state(state);
        let outcome = machine
            .transition(LifecycleTransitionTrigger::PhaseFailed {
                reason: "phase failed",
            })
            .expect("failure transition");
        let failure = outcome.failure().expect("failure fact");

        assert_eq!(failure.failed_state(), state);
        assert_eq!(failure.reason(), "phase failed");
        assert_storage_vocabulary(failure.reason());
        assert_eq!(machine.state(), LifecycleState::Failed);
        assert_eq!(machine.failure(), Some(failure));
        assert_allowed(
            machine.admit(LifecycleOperationKind::HealthQuery),
            LifecycleAdmissionEffect::Ordinary,
        );
        assert_rejected(machine.admit(LifecycleOperationKind::Commit));
    }

    let mut opening = machine_in_state(LifecycleState::Opening);
    assert_eq!(
        opening.transition(LifecycleTransitionTrigger::PhaseFailed { reason: "" }),
        Err(LifecycleError::InvalidLifecycleState {
            reason: "failure reason must not be empty",
        })
    );
    assert_eq!(opening.state(), LifecycleState::Opening);
    assert_eq!(opening.failure(), None);
}

#[test]
fn lifecycle_close_retry_and_closed_idempotence_are_distinct() {
    let mut machine = machine_in_state(LifecycleState::Open);
    let close_requested = machine
        .transition(LifecycleTransitionTrigger::CloseRequested)
        .expect("close requested");
    assert_eq!(
        close_requested.close_fact(),
        Some(LifecycleCloseFact::Requested)
    );
    assert_eq!(machine.state(), LifecycleState::Closing);

    let retry = machine
        .transition(LifecycleTransitionTrigger::CloseRetried)
        .expect("closing retry");
    assert_eq!(retry.to(), LifecycleState::Closing);
    assert!(!retry.is_idempotent());
    assert_eq!(retry.close_fact(), Some(LifecycleCloseFact::RetryPending));
    assert_eq!(machine.close_fact(), Some(LifecycleCloseFact::RetryPending));

    let completed = machine
        .transition(LifecycleTransitionTrigger::CloseCompleted)
        .expect("close completed");
    assert_eq!(completed.to(), LifecycleState::Closed);
    assert_eq!(completed.close_fact(), Some(LifecycleCloseFact::Complete));
    assert_eq!(machine.close_fact(), Some(LifecycleCloseFact::Complete));

    for _ in 0..2 {
        let idempotent = machine
            .transition(LifecycleTransitionTrigger::CloseRetried)
            .expect("closed retry");
        assert_eq!(idempotent.to(), LifecycleState::Closed);
        assert!(idempotent.is_idempotent());
        assert_eq!(
            idempotent.close_fact(),
            Some(LifecycleCloseFact::AlreadyClosed)
        );
    }

    let mut new = LifecycleStateMachine::new();
    assert_eq!(
        new.transition(LifecycleTransitionTrigger::CloseRetried),
        Err(LifecycleError::InvalidLifecycleState {
            reason: "invalid lifecycle transition",
        })
    );
}

#[test]
fn lifecycle_failed_from_closing_admits_close_and_retransitions_to_closing() {
    // A close that panics during, e.g., a transient sync hiccup leaves the
    // machine in Failed with `failed_state == Closing`. Retry-class close
    // must drive Failed -> Closing so the runtime can clean-shutdown
    // without leaking the writer guard via Drop.
    let mut machine = machine_in_state(LifecycleState::Closing);
    machine
        .transition(LifecycleTransitionTrigger::PhaseFailed {
            reason: "close failed mid-sync",
        })
        .expect("close-class failure");
    assert_eq!(machine.state(), LifecycleState::Failed);
    let failure = machine.failure().expect("failure fact");
    assert_eq!(failure.failed_state(), LifecycleState::Closing);

    // Admission honors the failure context: Failed-from-Closing admits Close
    // while Failed-from-Opening (covered below) rejects it.
    assert_allowed(
        machine.admit(LifecycleOperationKind::Close),
        LifecycleAdmissionEffect::Ordinary,
    );

    let outcome = machine
        .transition(LifecycleTransitionTrigger::CloseRequested)
        .expect("Failed -> Closing retry");
    assert_eq!(outcome.from(), LifecycleState::Failed);
    assert_eq!(outcome.to(), LifecycleState::Closing);
    assert_eq!(outcome.close_fact(), Some(LifecycleCloseFact::Requested));
    assert_eq!(machine.state(), LifecycleState::Closing);

    // The failure fact persists as breadcrumb; subsequent CloseCompleted
    // closes cleanly.
    let completed = machine
        .transition(LifecycleTransitionTrigger::CloseCompleted)
        .expect("retry close completes");
    assert_eq!(completed.to(), LifecycleState::Closed);
}

#[test]
fn lifecycle_failed_from_opening_rejects_close_requested_and_admit() {
    // Failures raised during Opening/Recovering are NOT owned by the
    // close path. Failed -> Closing must reject in that case so an
    // Open-time failure is not silently retried as if it were a close
    // failure.
    let mut machine = machine_in_state(LifecycleState::Opening);
    machine
        .transition(LifecycleTransitionTrigger::PhaseFailed {
            reason: "open failed before recovery",
        })
        .expect("open-class failure");
    assert_eq!(machine.state(), LifecycleState::Failed);
    let failure = machine.failure().expect("failure fact");
    assert_eq!(failure.failed_state(), LifecycleState::Opening);

    let admission = machine.admit(LifecycleOperationKind::Close);
    assert!(!admission.is_allowed(), "admit allowed unexpectedly: {admission:?}");
    assert_eq!(
        admission.rejection_reason(),
        Some("Failed admits Close only when the prior failure was raised during close"),
    );

    let snapshot = (machine.state(), machine.failure(), machine.close_fact());
    let error = machine
        .transition(LifecycleTransitionTrigger::CloseRequested)
        .expect_err("Failed-from-Opening must reject CloseRequested");
    assert_eq!(
        error,
        LifecycleError::InvalidLifecycleState {
            reason: "Failed -> Closing requires a prior close-class failure",
        }
    );
    // Rejected transitions must not mutate state.
    assert_eq!(
        (machine.state(), machine.failure(), machine.close_fact()),
        snapshot
    );
}

fn machine_in_state(state: LifecycleState) -> LifecycleStateMachine {
    let mut machine = LifecycleStateMachine::new();
    match state {
        LifecycleState::New => {}
        LifecycleState::Opening => {
            machine
                .transition(LifecycleTransitionTrigger::OpenRequested)
                .expect("opening");
        }
        LifecycleState::Recovering => {
            machine
                .transition(LifecycleTransitionTrigger::OpenRequested)
                .expect("opening");
            machine
                .transition(LifecycleTransitionTrigger::DurableRecoveryRequired)
                .expect("recovering");
        }
        LifecycleState::Open => {
            machine
                .transition(LifecycleTransitionTrigger::OpenRequested)
                .expect("opening");
            machine
                .transition(LifecycleTransitionTrigger::CacheOpenReady)
                .expect("open");
        }
        LifecycleState::Closing => {
            machine = machine_in_state(LifecycleState::Open);
            machine
                .transition(LifecycleTransitionTrigger::CloseRequested)
                .expect("closing");
        }
        LifecycleState::Closed => {
            machine = machine_in_state(LifecycleState::Closing);
            machine
                .transition(LifecycleTransitionTrigger::CloseCompleted)
                .expect("closed");
        }
        LifecycleState::Failed => {
            machine = machine_in_state(LifecycleState::Opening);
            machine
                .transition(LifecycleTransitionTrigger::PhaseFailed { reason: "failed" })
                .expect("failed");
        }
    }
    machine
}

fn assert_valid_transition(
    (start, trigger, expected, expected_close, expected_failure, idempotent): (
        LifecycleState,
        LifecycleTransitionTrigger,
        LifecycleState,
        Option<LifecycleCloseFact>,
        Option<LifecycleState>,
        bool,
    ),
) {
    let mut machine = machine_in_state(start);
    let outcome = machine.transition(trigger).expect("valid transition");

    assert_eq!(outcome.from(), start);
    assert_eq!(outcome.to(), expected);
    assert_eq!(outcome.trigger(), trigger);
    assert_eq!(
        outcome.effect(),
        if idempotent {
            LifecycleTransitionEffect::Idempotent
        } else {
            LifecycleTransitionEffect::Applied
        }
    );
    assert_eq!(outcome.close_fact(), expected_close);
    assert_eq!(outcome.is_idempotent(), idempotent);
    assert_eq!(machine.state(), expected);
    match expected_failure {
        Some(failed_state) => {
            let failure = outcome.failure().expect("failure fact");
            assert_eq!(failure.failed_state(), failed_state);
            assert!(!failure.reason().is_empty());
            assert_eq!(machine.failure(), Some(failure));
        }
        None => assert_eq!(outcome.failure(), None),
    }
}

fn expected_admission(
    state: LifecycleState,
    operation: LifecycleOperationKind,
) -> LifecycleOperationAdmission {
    match (state, operation) {
        (_, LifecycleOperationKind::HealthQuery)
        | (LifecycleState::New, LifecycleOperationKind::Open)
        | (LifecycleState::Recovering, LifecycleOperationKind::RecoveryStep)
        | (
            LifecycleState::Open,
            LifecycleOperationKind::OrdinaryRead
            | LifecycleOperationKind::Commit
            | LifecycleOperationKind::OrdinaryMaintenance
            | LifecycleOperationKind::Close,
        )
        | (
            LifecycleState::Closing,
            LifecycleOperationKind::CloseRequiredDrain | LifecycleOperationKind::CloseRetry,
        ) => LifecycleOperationAdmission::Allowed {
            effect: LifecycleAdmissionEffect::Ordinary,
        },
        (
            LifecycleState::Closed,
            LifecycleOperationKind::Close | LifecycleOperationKind::CloseRetry,
        ) => LifecycleOperationAdmission::Allowed {
            effect: LifecycleAdmissionEffect::Idempotent,
        },
        _ => LifecycleOperationAdmission::Rejected {
            reason: "operation is not admitted in current lifecycle state",
        },
    }
}

fn assert_allowed(
    admission: LifecycleOperationAdmission,
    expected_effect: LifecycleAdmissionEffect,
) {
    assert!(admission.is_allowed(), "admission rejected: {admission:?}");
    assert_eq!(
        admission,
        LifecycleOperationAdmission::Allowed {
            effect: expected_effect,
        }
    );
    assert_eq!(
        admission.is_idempotent(),
        matches!(expected_effect, LifecycleAdmissionEffect::Idempotent)
    );
}

fn assert_rejected(admission: LifecycleOperationAdmission) {
    assert!(!admission.is_allowed(), "admission allowed: {admission:?}");
    assert_eq!(
        admission.rejection_reason(),
        Some("operation is not admitted in current lifecycle state")
    );
}
