//! Side-effect-free lifecycle state transitions and admission checks.

use super::{LifecycleError, LifecycleResult, LifecycleState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleStateMachine {
    state: LifecycleState,
    failure: Option<LifecycleFailureFact>,
    close_fact: Option<LifecycleCloseFact>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleTransitionTrigger {
    OpenRequested,
    CacheOpenReady,
    DurableRecoveryRequired,
    RecoveryAccepted,
    CloseRequested,
    CloseCompleted,
    CloseRetried,
    PhaseFailed { reason: &'static str },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleTransitionOutcome {
    from: LifecycleState,
    to: LifecycleState,
    trigger: LifecycleTransitionTrigger,
    effect: LifecycleTransitionEffect,
    failure: Option<LifecycleFailureFact>,
    close_fact: Option<LifecycleCloseFact>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleTransitionEffect {
    Applied,
    Idempotent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleFailureFact {
    failed_state: LifecycleState,
    reason: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleCloseFact {
    Requested,
    RetryPending,
    Complete,
    AlreadyClosed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleOperationKind {
    Open,
    OrdinaryRead,
    Commit,
    RecoveryStep,
    OrdinaryMaintenance,
    CloseRequiredDrain,
    HealthQuery,
    Close,
    CloseRetry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleOperationAdmission {
    Allowed { effect: LifecycleAdmissionEffect },
    Rejected { reason: &'static str },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleAdmissionEffect {
    Ordinary,
    Idempotent,
}

impl LifecycleStateMachine {
    pub(crate) const fn new() -> Self {
        Self {
            state: LifecycleState::New,
            failure: None,
            close_fact: None,
        }
    }

    pub(crate) const fn state(self) -> LifecycleState {
        self.state
    }

    pub(crate) const fn failure(self) -> Option<LifecycleFailureFact> {
        self.failure
    }

    pub(crate) const fn close_fact(self) -> Option<LifecycleCloseFact> {
        self.close_fact
    }

    pub(crate) fn transition(
        &mut self,
        trigger: LifecycleTransitionTrigger,
    ) -> LifecycleResult<LifecycleTransitionOutcome> {
        let from = self.state;
        let update = transition_update(from, trigger, self.failure)?;
        self.state = update.to;
        if let Some(failure) = update.failure {
            self.failure = Some(failure);
        }
        if let Some(close_fact) = update.close_fact {
            self.close_fact = Some(close_fact);
        }
        // Failed -> Closing leaves the failure fact in place as breadcrumb
        // but the transition was the retry-class one; nothing else changes.
        Ok(LifecycleTransitionOutcome {
            from,
            to: update.to,
            trigger,
            effect: update.effect,
            failure: update.failure,
            close_fact: update.close_fact,
        })
    }

    pub(crate) const fn admit(
        self,
        operation: LifecycleOperationKind,
    ) -> LifecycleOperationAdmission {
        // Failed admits Close only when the prior failure happened during
        // close-class work, mirroring the matching transition rule.
        match (self.state, operation) {
            (LifecycleState::Failed, LifecycleOperationKind::Close) => match self.failure {
                Some(fact) if matches!(fact.failed_state(), LifecycleState::Closing) => allow(),
                _ => reject(
                    "Failed admits Close only when the prior failure was raised during close",
                ),
            },
            _ => Self::admit_state(self.state, operation),
        }
    }

    pub(crate) const fn admit_state(
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
            ) => allow(),
            (
                LifecycleState::Closed,
                LifecycleOperationKind::Close | LifecycleOperationKind::CloseRetry,
            ) => allow_idempotent(),
            _ => reject("operation is not admitted in current lifecycle state"),
        }
    }
}

impl LifecycleTransitionOutcome {
    pub(crate) const fn from(self) -> LifecycleState {
        self.from
    }

    pub(crate) const fn to(self) -> LifecycleState {
        self.to
    }

    pub(crate) const fn trigger(self) -> LifecycleTransitionTrigger {
        self.trigger
    }

    pub(crate) const fn effect(self) -> LifecycleTransitionEffect {
        self.effect
    }

    pub(crate) const fn is_idempotent(self) -> bool {
        matches!(self.effect, LifecycleTransitionEffect::Idempotent)
    }

    pub(crate) const fn failure(self) -> Option<LifecycleFailureFact> {
        self.failure
    }

    pub(crate) const fn close_fact(self) -> Option<LifecycleCloseFact> {
        self.close_fact
    }
}

impl LifecycleFailureFact {
    pub(crate) const fn new(
        failed_state: LifecycleState,
        reason: &'static str,
    ) -> LifecycleResult<Self> {
        if reason.is_empty() {
            return Err(LifecycleError::InvalidLifecycleState {
                reason: "failure reason must not be empty",
            });
        }
        Ok(Self {
            failed_state,
            reason,
        })
    }

    pub(crate) const fn failed_state(self) -> LifecycleState {
        self.failed_state
    }

    pub(crate) const fn reason(self) -> &'static str {
        self.reason
    }
}

impl LifecycleOperationAdmission {
    pub(crate) const fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed { .. })
    }

    pub(crate) const fn is_idempotent(self) -> bool {
        matches!(
            self,
            Self::Allowed {
                effect: LifecycleAdmissionEffect::Idempotent
            }
        )
    }

    pub(crate) const fn rejection_reason(self) -> Option<&'static str> {
        match self {
            Self::Rejected { reason } => Some(reason),
            Self::Allowed { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TransitionUpdate {
    to: LifecycleState,
    effect: LifecycleTransitionEffect,
    failure: Option<LifecycleFailureFact>,
    close_fact: Option<LifecycleCloseFact>,
}

fn transition_update(
    from: LifecycleState,
    trigger: LifecycleTransitionTrigger,
    failure: Option<LifecycleFailureFact>,
) -> LifecycleResult<TransitionUpdate> {
    let update = match (from, trigger) {
        (LifecycleState::New, LifecycleTransitionTrigger::OpenRequested) => {
            transition_to(LifecycleState::Opening)
        }
        (LifecycleState::Opening, LifecycleTransitionTrigger::CacheOpenReady)
        | (LifecycleState::Recovering, LifecycleTransitionTrigger::RecoveryAccepted) => {
            transition_to(LifecycleState::Open)
        }
        (LifecycleState::Opening, LifecycleTransitionTrigger::DurableRecoveryRequired) => {
            transition_to(LifecycleState::Recovering)
        }
        (
            LifecycleState::Opening | LifecycleState::Recovering | LifecycleState::Closing,
            LifecycleTransitionTrigger::PhaseFailed { reason },
        ) => {
            let failure = LifecycleFailureFact::new(from, reason)?;
            TransitionUpdate {
                to: LifecycleState::Failed,
                effect: LifecycleTransitionEffect::Applied,
                failure: Some(failure),
                close_fact: None,
            }
        }
        (LifecycleState::Open, LifecycleTransitionTrigger::CloseRequested) => TransitionUpdate {
            to: LifecycleState::Closing,
            effect: LifecycleTransitionEffect::Applied,
            failure: None,
            close_fact: Some(LifecycleCloseFact::Requested),
        },
        // Failed -> Closing is only allowed when the failure was raised
        // during close-class work (`failed_state == Closing`). That confines
        // the recovery path to close-owned failures — a partial close that
        // hit, e.g. a transient WAL sync hiccup — and prevents Open/Recovering
        // failures from being silently retried through the close ordering.
        (LifecycleState::Failed, LifecycleTransitionTrigger::CloseRequested) => match failure {
            Some(fact) if fact.failed_state() == LifecycleState::Closing => TransitionUpdate {
                to: LifecycleState::Closing,
                effect: LifecycleTransitionEffect::Applied,
                failure: None,
                close_fact: Some(LifecycleCloseFact::Requested),
            },
            _ => {
                return Err(LifecycleError::InvalidLifecycleState {
                    reason: "Failed -> Closing requires a prior close-class failure",
                });
            }
        },
        (LifecycleState::Closing, LifecycleTransitionTrigger::CloseCompleted) => TransitionUpdate {
            to: LifecycleState::Closed,
            effect: LifecycleTransitionEffect::Applied,
            failure: None,
            close_fact: Some(LifecycleCloseFact::Complete),
        },
        (LifecycleState::Closing, LifecycleTransitionTrigger::CloseRetried) => TransitionUpdate {
            to: LifecycleState::Closing,
            effect: LifecycleTransitionEffect::Applied,
            failure: None,
            close_fact: Some(LifecycleCloseFact::RetryPending),
        },
        (LifecycleState::Closed, LifecycleTransitionTrigger::CloseRetried) => TransitionUpdate {
            to: LifecycleState::Closed,
            effect: LifecycleTransitionEffect::Idempotent,
            failure: None,
            close_fact: Some(LifecycleCloseFact::AlreadyClosed),
        },
        _ => {
            return Err(LifecycleError::InvalidLifecycleState {
                reason: "invalid lifecycle transition",
            });
        }
    };
    Ok(update)
}

const fn transition_to(to: LifecycleState) -> TransitionUpdate {
    TransitionUpdate {
        to,
        effect: LifecycleTransitionEffect::Applied,
        failure: None,
        close_fact: None,
    }
}

const fn allow() -> LifecycleOperationAdmission {
    LifecycleOperationAdmission::Allowed {
        effect: LifecycleAdmissionEffect::Ordinary,
    }
}

const fn allow_idempotent() -> LifecycleOperationAdmission {
    LifecycleOperationAdmission::Allowed {
        effect: LifecycleAdmissionEffect::Idempotent,
    }
}

const fn reject(reason: &'static str) -> LifecycleOperationAdmission {
    LifecycleOperationAdmission::Rejected { reason }
}
