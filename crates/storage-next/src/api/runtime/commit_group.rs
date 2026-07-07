//! Write-group join queue (BS5.1).
//!
//! Every durable commit joins the queue. The first joiner while no leader is
//! active becomes the leader: it takes the runtime lock, drains up to
//! [`COMMIT_GROUP_MAX_MEMBERS`] queued requests, and executes them as one
//! write group (one durable-gate span, one covering fsync, one visible
//! publish). Waiting members park on their own joiner's condvar — never on
//! the runtime lock — so the moment the leader publishes their responses they
//! wake, return, and re-join, keeping the next group full. On finish the
//! leader promotes the queue's front joiner as the next leader (or clears the
//! leadership flag when the queue is empty); a timed-wait fallback lets a
//! waiter assume leadership if a promotion was lost, so liveness never
//! depends on a single wake-up.
//!
//! Single-threaded callers (and wasm, per constraint C1) never wait: joining
//! an empty queue with no active leader returns leadership immediately — the
//! exact solo commit path with one uncontended queue-mutex hop.

use crate::commit::{CommitBatch, CommitBranchGenerationGuard, CommitOutcome};
use crate::lifecycle::{LifecycleError, LifecycleWalGrowthOutcome, LifecycleWriteAdmissionOutcome};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Maximum members per write group, the leader included (plan D3).
pub(super) const COMMIT_GROUP_MAX_MEMBERS: usize = 16;

/// Fallback poll interval for waiters (lost-promotion safety net only; the
/// normal path is a direct notify).
const WAITER_FALLBACK_POLL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub(super) struct CommitGroupRequest {
    pub(super) batch: CommitBatch,
    pub(super) generation_guard: CommitBranchGenerationGuard,
}

/// Everything a caller needs after the under-lock section: the outcome plus
/// the same per-caller snapshots the solo path reads under the lock.
#[derive(Debug)]
pub(super) struct CommitGroupResponse {
    pub(super) outcome: Result<CommitOutcome, LifecycleError>,
    pub(super) admission: Option<LifecycleWriteAdmissionOutcome>,
    pub(super) pending_tasks: usize,
    pub(super) wal_growth: Option<LifecycleWalGrowthOutcome>,
    pub(super) throttle_delay_millis: u64,
}

#[derive(Debug)]
enum JoinState {
    /// Queued, waiting for a leader (request still inside).
    Pending(CommitGroupRequest),
    /// A leader took the request and is executing it.
    Taken,
    /// Promoted to lead: the waiter takes its request back and leads.
    Promoted(CommitGroupRequest),
    // Boxed: the response (~760 bytes) dwarfs the other variants, and each
    // joiner completes at most once.
    Done(Box<CommitGroupResponse>),
}

#[derive(Debug)]
pub(super) struct CommitGroupJoiner {
    state: Mutex<JoinState>,
    ready: Condvar,
}

impl CommitGroupJoiner {
    fn new(request: CommitGroupRequest) -> Self {
        Self {
            state: Mutex::new(JoinState::Pending(request)),
            ready: Condvar::new(),
        }
    }

    /// Leader-side: take the request for execution (`Pending` → `Taken`).
    /// Returns `None` for stale nodes (already taken, promoted, or done).
    fn take_request(&self) -> Option<CommitGroupRequest> {
        // Rationale: a poisoned joiner mutex means the owning caller panicked;
        // skipping the node fails closed (its caller already returned).
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        match &*state {
            JoinState::Pending(_) => match std::mem::replace(&mut *state, JoinState::Taken) {
                JoinState::Pending(request) => Some(request),
                // Unreachable: the match arm above proved the state Pending
                // while the lock is held.
                _ => None,
            },
            _ => None,
        }
    }

    /// Leader-side: publish the member's response and wake its caller.
    pub(super) fn complete(&self, response: CommitGroupResponse) {
        if let Ok(mut state) = self.state.lock() {
            *state = JoinState::Done(Box::new(response));
        }
        // Rationale: a poisoned joiner mutex means the owning caller panicked
        // and will never read the response; dropping it is the only option.
        self.ready.notify_one();
    }

    /// Promotion: hand leadership to this (still pending) joiner. Returns
    /// false for stale nodes.
    fn promote(&self) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if !matches!(&*state, JoinState::Pending(_)) {
            return false;
        }
        let JoinState::Pending(request) = std::mem::replace(&mut *state, JoinState::Taken) else {
            // Unreachable: checked Pending under the same lock hold.
            return false;
        };
        *state = JoinState::Promoted(request);
        drop(state);
        self.ready.notify_one();
        true
    }
}

/// Where a joining commit goes: lead now, or wait to be served.
#[derive(Debug)]
pub(super) enum JoinPath {
    /// No leader was active: the caller leads immediately (its request never
    /// enters the queue). The guard hands leadership on when dropped.
    Lead(CommitGroupRequest),
    /// A leader is active: wait on the joiner for a response or a promotion.
    Wait(Arc<CommitGroupJoiner>),
}

/// Terminal state a waiting caller observes.
#[derive(Debug)]
pub(super) enum WaitOutcome {
    /// A leader executed this request; the response is ready.
    Done(Box<CommitGroupResponse>),
    /// Promoted (or self-promoted after a lost wake-up): lead with the
    /// returned request.
    Lead(CommitGroupRequest),
    /// The request was taken but never completed. Leaders complete every
    /// taken member before handing off, so this is only reachable if a leader
    /// died mid-group — fail closed, never re-execute.
    Abandoned,
}

#[derive(Debug, Default)]
struct QueueState {
    queue: VecDeque<Arc<CommitGroupJoiner>>,
    /// True while some caller owns leadership (executing or promoted). At most
    /// one leader exists at a time; it drains the queue the members wait in.
    leader_active: bool,
}

#[derive(Debug, Default)]
pub(super) struct CommitGroupQueue {
    state: Mutex<QueueState>,
    /// Set whenever a commit had to wait (a leader was active). A leader that
    /// drains an empty queue consumes it to decide whether a brief formation
    /// window is worth waiting: just-served members are typically re-joining
    /// within microseconds of the handoff, and catching them turns a
    /// serialized solo round into a full group. Never set single-threaded, so
    /// uncontended commits never pay the window.
    recent_contention: std::sync::atomic::AtomicBool,
}

impl CommitGroupQueue {
    /// Join the write-group protocol: become the leader if none is active,
    /// otherwise enqueue and wait.
    pub(super) fn join(&self, request: CommitGroupRequest) -> JoinPath {
        // Rationale: queue-mutex poisoning requires a panic inside the tiny
        // sections of this module; leading solo is the fail-closed fallback
        // (the commit still executes with full solo semantics).
        let Ok(mut state) = self.state.lock() else {
            return JoinPath::Lead(request);
        };
        if !state.leader_active {
            state.leader_active = true;
            return JoinPath::Lead(request);
        }
        let joiner = Arc::new(CommitGroupJoiner::new(request));
        state.queue.push_back(Arc::clone(&joiner));
        self.recent_contention
            .store(true, std::sync::atomic::Ordering::Release);
        JoinPath::Wait(joiner)
    }

    /// Leader-side: consume the recent-contention signal (see the field doc).
    pub(super) fn take_recent_contention(&self) -> bool {
        self.recent_contention
            .swap(false, std::sync::atomic::Ordering::AcqRel)
    }

    /// Leader-side: a multi-member group was just served — its members are
    /// about to re-join, so the next leader should hold the formation window
    /// open for them even if none has re-joined yet (their wake-up races the
    /// leadership handoff).
    pub(super) fn note_group_served(&self) {
        self.recent_contention
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Leader-side: drain up to `max` pending joiners with their requests, in
    /// FIFO order. Stale nodes are discarded without counting.
    pub(super) fn drain_members(
        &self,
        max: usize,
    ) -> Vec<(Arc<CommitGroupJoiner>, CommitGroupRequest)> {
        let mut members = Vec::new();
        let Ok(mut state) = self.state.lock() else {
            return members;
        };
        while members.len() < max {
            let Some(joiner) = state.queue.pop_front() else {
                break;
            };
            if let Some(request) = joiner.take_request() {
                members.push((joiner, request));
            }
        }
        members
    }

    /// Leader-side: hand leadership on. Promotes the queue's front pending
    /// joiner as the next leader, or clears the leadership flag when nothing
    /// is queued. Every leader MUST call this exactly once (the API layer
    /// wraps it in a drop guard so a panicking leader still hands off).
    pub(super) fn finish_leadership(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        while let Some(joiner) = state.queue.pop_front() {
            if joiner.promote() {
                // Leadership transfers: the flag stays set for the promotee.
                return;
            }
        }
        state.leader_active = false;
    }

    /// Waiter-side fallback: claim leadership if none is active (covers a
    /// lost promotion). On success the waiter must lead with its own request.
    fn try_assume_leadership(&self) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if state.leader_active {
            return false;
        }
        state.leader_active = true;
        true
    }

    /// Waiter-side: block until served or promoted. Uses a timed wait purely
    /// as a lost-wake-up safety net; the normal path is a direct notify from
    /// the leader.
    pub(super) fn await_service(&self, joiner: &CommitGroupJoiner) -> WaitOutcome {
        let Ok(mut state) = joiner.state.lock() else {
            return WaitOutcome::Abandoned;
        };
        loop {
            match &*state {
                // Pending: no leader has reached us yet. Taken: a leader is
                // executing our request right now (possibly a long fsync) —
                // keep waiting for its completion in both cases.
                JoinState::Pending(_) | JoinState::Taken => {}
                JoinState::Promoted(_) | JoinState::Done(_) => {
                    match std::mem::replace(&mut *state, JoinState::Taken) {
                        JoinState::Promoted(request) => return WaitOutcome::Lead(request),
                        JoinState::Done(response) => return WaitOutcome::Done(response),
                        // Unreachable: matched Promoted/Done under this hold.
                        _ => return WaitOutcome::Abandoned,
                    }
                }
            }
            let Ok((next, timeout)) = joiner.ready.wait_timeout(state, WAITER_FALLBACK_POLL) else {
                return WaitOutcome::Abandoned;
            };
            state = next;
            if timeout.timed_out() && matches!(&*state, JoinState::Pending(_)) {
                // Nobody woke us within the fallback window. If no leader is
                // active (a promotion was lost), take over with our own
                // request; otherwise keep waiting for the active leader.
                drop(state);
                if self.try_assume_leadership() {
                    let Ok(mut reclaimed) = joiner.state.lock() else {
                        // Release the just-claimed leadership before failing
                        // closed, or no later joiner could ever lead.
                        self.finish_leadership();
                        return WaitOutcome::Abandoned;
                    };
                    match std::mem::replace(&mut *reclaimed, JoinState::Taken) {
                        JoinState::Pending(request) => return WaitOutcome::Lead(request),
                        JoinState::Done(response) => {
                            drop(reclaimed);
                            self.finish_leadership();
                            return WaitOutcome::Done(response);
                        }
                        _ => {
                            drop(reclaimed);
                            self.finish_leadership();
                            return WaitOutcome::Abandoned;
                        }
                    }
                }
                let Ok(again) = joiner.state.lock() else {
                    return WaitOutcome::Abandoned;
                };
                state = again;
            }
        }
    }
}

/// Drop guard binding a leadership span to a scope: created when a caller
/// becomes leader, hands leadership on even if the leader's execution panics.
#[derive(Debug)]
pub(super) struct CommitGroupLeadership<'q> {
    queue: &'q CommitGroupQueue,
}

impl<'q> CommitGroupLeadership<'q> {
    pub(super) const fn new(queue: &'q CommitGroupQueue) -> Self {
        Self { queue }
    }
}

impl Drop for CommitGroupLeadership<'_> {
    fn drop(&mut self) {
        self.queue.finish_leadership();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::{
        CommitBatch, CommitBatchOptions, CommitConflictValidationMode, CommitDuplicateKeyPolicy,
        CommitDurabilityMode, CommitExpiry, CommitMutation, CommitOrigin, CommitRetentionHint,
        CommitTimestampPolicy, CommitValidationFacts,
    };
    use crate::row::{PhysicalKey, StorageSpaceId};
    use strata_core_next::BranchId;

    fn request(tag: u8) -> CommitGroupRequest {
        let branch = BranchId::from_bytes([tag; 16]);
        CommitGroupRequest {
            batch: CommitBatch::mutating(
                branch,
                vec![CommitMutation::put(
                    PhysicalKey::new(
                        branch,
                        "join-queue",
                        StorageSpaceId::engine(0x37).expect("engine storage space"),
                        vec![tag],
                    )
                    .expect("physical key"),
                    vec![tag],
                    CommitExpiry::None,
                    CommitRetentionHint::Append,
                )],
                CommitValidationFacts::empty(),
                CommitBatchOptions::new(
                    CommitDurabilityMode::Standard,
                    CommitConflictValidationMode::Validate,
                    CommitDuplicateKeyPolicy::Reject,
                    CommitTimestampPolicy::RuntimeGenerated,
                    CommitOrigin::StorageRuntime,
                ),
            ),
            generation_guard: CommitBranchGenerationGuard::not_supplied(),
        }
    }

    fn response() -> CommitGroupResponse {
        CommitGroupResponse {
            outcome: Err(LifecycleError::InvalidLifecycleState {
                reason: "join-queue unit test response",
            }),
            admission: None,
            pending_tasks: 0,
            wal_growth: None,
            throttle_delay_millis: 0,
        }
    }

    fn request_tag(request: &CommitGroupRequest) -> u8 {
        request.batch.branch_id().as_bytes()[0]
    }

    #[test]
    fn first_joiner_leads_and_later_joiners_wait_in_fifo_order() {
        let queue = CommitGroupQueue::default();
        let JoinPath::Lead(leader) = queue.join(request(1)) else {
            panic!("first joiner must lead");
        };
        assert_eq!(request_tag(&leader), 1);

        let mut waiters = Vec::new();
        for tag in 2..=6 {
            match queue.join(request(tag)) {
                JoinPath::Wait(joiner) => waiters.push(joiner),
                JoinPath::Lead(_) => panic!("leader already active"),
            }
        }

        // Leader drains in FIFO order, capped.
        let first = queue.drain_members(3);
        let tags: Vec<_> = first
            .iter()
            .map(|(_, request)| request_tag(request))
            .collect();
        assert_eq!(tags, vec![2, 3, 4]);
        let rest = queue.drain_members(16);
        let tags: Vec<_> = rest
            .iter()
            .map(|(_, request)| request_tag(request))
            .collect();
        assert_eq!(tags, vec![5, 6]);
        assert!(queue.drain_members(16).is_empty());

        // Handing leadership off with an empty queue clears the flag: the
        // next joiner leads again.
        queue.finish_leadership();
        assert!(matches!(queue.join(request(7)), JoinPath::Lead(_)));
    }

    #[test]
    fn finish_leadership_promotes_the_front_waiter() {
        let queue = CommitGroupQueue::default();
        let JoinPath::Lead(_leader) = queue.join(request(1)) else {
            panic!("first joiner must lead");
        };
        let JoinPath::Wait(waiter) = queue.join(request(2)) else {
            panic!("second joiner must wait");
        };

        // The leader finishes without draining the waiter: the waiter is
        // promoted and leads with its own request.
        queue.finish_leadership();
        match queue.await_service(&waiter) {
            WaitOutcome::Lead(request) => assert_eq!(request_tag(&request), 2),
            outcome => panic!("expected promotion, got {outcome:?}"),
        }
        // Leadership stayed active across the promotion: new joiners wait.
        assert!(matches!(queue.join(request(3)), JoinPath::Wait(_)));
    }

    #[test]
    fn served_waiters_get_their_response_and_stale_nodes_are_skipped() {
        let queue = CommitGroupQueue::default();
        let JoinPath::Lead(_leader) = queue.join(request(1)) else {
            panic!("first joiner must lead");
        };
        let JoinPath::Wait(served) = queue.join(request(2)) else {
            panic!("waiter expected");
        };
        let JoinPath::Wait(_other) = queue.join(request(3)) else {
            panic!("waiter expected");
        };

        let members = queue.drain_members(1);
        assert_eq!(members.len(), 1);
        assert_eq!(request_tag(&members[0].1), 2);
        members[0].0.complete(response());
        match queue.await_service(&served) {
            WaitOutcome::Done(response) => assert!(response.outcome.is_err()),
            outcome => panic!("expected served response, got {outcome:?}"),
        }

        // A drained-and-completed node never reappears; the remaining pending
        // node is promoted on handoff.
        queue.finish_leadership();
        assert!(matches!(queue.join(request(4)), JoinPath::Wait(_)));
    }

    #[test]
    fn leadership_guard_hands_off_on_drop() {
        let queue = CommitGroupQueue::default();
        let JoinPath::Lead(_leader) = queue.join(request(1)) else {
            panic!("first joiner must lead");
        };
        {
            let _guard = CommitGroupLeadership::new(&queue);
            // Guard dropped here without an explicit finish.
        }
        assert!(matches!(queue.join(request(2)), JoinPath::Lead(_)));
    }
}
