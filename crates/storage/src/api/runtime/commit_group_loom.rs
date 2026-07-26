//! TCP4.3 — exhaustive schedule exploration of the write-group protocol
//! (`commit_group.rs`) under [loom]. Only compiled with
//! `RUSTFLAGS="--cfg loom"`; run via `cargo test -p strata-storage --lib
//! loom_`.
//!
//! These models drive the REAL protocol code (queue, joiner state machine,
//! apply exchange, sync chain) through the [`GroupProtocol`] seam with
//! lightweight payloads — not a hand-copied abstraction. What loom proves,
//! per explored schedule set:
//!
//! - **Safety**: at most one leader at a time; every request executed
//!   exactly once; responses reach their own joiner; the apply barrier
//!   loses no outcome; no caller leaves the sync chain uncovered.
//! - **Liveness**: loom's `Condvar::wait_timeout` NEVER times out
//!   (upstream TODO), so the product's timed fallbacks are unreachable
//!   here. A schedule that needs one to make progress therefore surfaces
//!   as a loom-detected deadlock — exactly the right posture: the direct
//!   notify protocol must be complete on its own, with the timed waits as
//!   safety nets, never load-bearing.
//! - **The #2682 shape**: a member-thread apply handed off through the
//!   real exchange, the leader publishing the visible frontier only after
//!   the barrier, and a concurrent frontier-bounded reader — the model
//!   asserts a reader that observes `V >= v` sees EVERY row of batch `v`
//!   (no torn read). The sabotage twin publishes before the barrier and
//!   must be caught, pinning the oracle non-vacuous.

use super::commit_group::{
    ExchangeHandle, GroupJoinPath, GroupLeadership, GroupProtocol, GroupQueue, GroupWaitOutcome,
    SyncTicket, WalSyncChain,
};
use loom::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Minimal protocol: request/response are tags, apply adds 1000.
#[derive(Debug)]
struct TagProtocol;

impl GroupProtocol for TagProtocol {
    type Request = u32;
    type Response = u32;
    type Work = u32;
    type Done = u32;

    fn apply(work: u32) -> u32 {
        work + 1000
    }
}

/// Shared truth for the queue model: per-tag execution counts plus the
/// active-leader gauge.
struct QueueTruth {
    executed: [AtomicUsize; 4],
    active_leaders: AtomicUsize,
}

impl QueueTruth {
    fn new() -> Self {
        Self {
            executed: [
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
            ],
            active_leaders: AtomicUsize::new(0),
        }
    }
}

/// One leadership span, exactly as `mod.rs` runs it: execute the leader's
/// own request, drain members, complete each with `tag * 10`, hand off via
/// the drop guard.
fn lead(queue: &GroupQueue<TagProtocol>, own: u32, truth: &QueueTruth) {
    let before = truth.active_leaders.fetch_add(1, Ordering::SeqCst);
    assert_eq!(before, 0, "two leaders active at once");
    let leadership = GroupLeadership::new(queue);
    truth.executed[own as usize].fetch_add(1, Ordering::SeqCst);
    for (handle, request) in queue.drain_members(16) {
        truth.executed[request as usize].fetch_add(1, Ordering::SeqCst);
        handle.complete(request * 10);
    }
    truth.active_leaders.fetch_sub(1, Ordering::SeqCst);
    drop(leadership);
}

/// Join with `tag` and follow the protocol to termination, whatever the
/// schedule dealt (serve, promotion, or immediate leadership).
fn join_to_completion(queue: &GroupQueue<TagProtocol>, tag: u32, truth: &QueueTruth) {
    match queue.join(tag) {
        GroupJoinPath::Lead(own) => lead(queue, own, truth),
        GroupJoinPath::Wait(handle) => match queue.await_service(&handle) {
            GroupWaitOutcome::Done(response) => {
                assert_eq!(
                    *response,
                    tag * 10,
                    "response delivered to the wrong joiner"
                );
            }
            GroupWaitOutcome::Lead(own) => {
                assert_eq!(own, tag, "promotion returned someone else's request");
                lead(queue, own, truth);
            }
            GroupWaitOutcome::Abandoned => panic!("no leader died, yet a joiner was abandoned"),
        },
    }
}

/// Three concurrent writers: exactly one leader at a time, every request
/// executed exactly once, every joiner served or promoted — across every
/// schedule loom can reach at this bound.
#[test]
fn loom_queue_single_leader_and_exactly_once_execution() {
    let mut model = loom::model::Builder::new();
    model.preemption_bound = Some(3);
    model.check(|| {
        let queue = Arc::new(GroupQueue::<TagProtocol>::default());
        let truth = Arc::new(QueueTruth::new());

        let handles: Vec<_> = [1_u32, 2, 3]
            .into_iter()
            .map(|tag| {
                let queue = Arc::clone(&queue);
                let truth = Arc::clone(&truth);
                thread::spawn(move || join_to_completion(&queue, tag, &truth))
            })
            .collect();
        for handle in handles {
            handle.join().expect("writer thread completes");
        }

        for tag in 1..=3 {
            assert_eq!(
                truth.executed[tag].load(Ordering::SeqCst),
                1,
                "request {tag} must execute exactly once"
            );
        }
        assert_eq!(truth.active_leaders.load(Ordering::SeqCst), 0);
    });
}

/// The BS5.4c handoff: the leader hands a parked member its apply through
/// the real exchange; the barrier must collect the outcome and the member
/// must still receive its own response — under every interleaving of the
/// handoff, the member's wait loop, and completion.
#[test]
fn loom_apply_handoff_barrier_loses_nothing() {
    loom::model(|| {
        let queue = Arc::new(GroupQueue::<TagProtocol>::default());

        // Main is the leader; the member's joiner is created before the
        // member thread starts so the drain below cannot miss it (handles
        // are shared state — which thread parks on one is free).
        let GroupJoinPath::Lead(_own) = queue.join(0) else {
            panic!("first joiner must lead");
        };
        let leadership = GroupLeadership::new(&queue);
        let GroupJoinPath::Wait(handle) = queue.join(7) else {
            panic!("leader is active; the member must wait");
        };

        let member = {
            let queue = Arc::clone(&queue);
            let handle = handle.clone();
            thread::spawn(move || match queue.await_service(&handle) {
                GroupWaitOutcome::Done(response) => *response,
                outcome => panic!("member expected service, got {outcome:?}"),
            })
        };

        let members = queue.drain_members(16);
        assert_eq!(members.len(), 1, "the member's request is queued");
        let (member_handle, request) = &members[0];
        assert_eq!(*request, 7);

        let exchange = ExchangeHandle::<TagProtocol>::default();
        match member_handle.request_apply(5, &exchange) {
            Ok(()) => {
                // The member executes `apply(5)` on its own thread; the
                // barrier must deliver it.
                assert_eq!(exchange.wait_for(1), vec![1005], "apply outcome lost");
            }
            Err(work) => {
                // The member was not parked in Taken yet in this schedule:
                // the leader takes the work back and applies itself (the
                // mod.rs path).
                assert_eq!(work, 5);
            }
        }

        member_handle.complete(70);
        drop(leadership);
        assert_eq!(member.join().expect("member completes"), 70);
    });
}

/// Instrumented ticket for the covering-fsync chain: `sync()` asserts the
/// single-flight invariant and publishes nothing itself (the chain owns the
/// watermark publish).
#[derive(Debug)]
struct TestTicket {
    captured: u64,
    in_flight: Arc<AtomicUsize>,
}

impl SyncTicket for TestTicket {
    type Error = std::convert::Infallible;

    fn captured_seq(&self) -> u64 {
        self.captured
    }

    fn sync(&self) -> Result<(), Self::Error> {
        let overlapping = self.in_flight.fetch_add(1, Ordering::SeqCst);
        assert_eq!(overlapping, 0, "two covering fsyncs in flight at once");
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}

/// Two pipelined groups racing the covering-fsync chain: whatever the
/// schedule, no caller returns while its own appends are uncovered, and no
/// two syncs ever overlap. (The durable watermark is a std atomic by
/// design — the chain's parameter type; loom still explores every
/// mutex/condvar schedule around it.)
#[test]
fn loom_sync_chain_never_returns_uncovered() {
    loom::model(|| {
        let chain = Arc::new(WalSyncChain::default());
        let appended = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let durable = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let in_flight = Arc::new(AtomicUsize::new(0));

        let workers: Vec<_> = [1_u64, 2]
            .into_iter()
            .map(|seq| {
                let chain = Arc::clone(&chain);
                let appended = Arc::clone(&appended);
                let durable = Arc::clone(&durable);
                let in_flight = Arc::clone(&in_flight);
                thread::spawn(move || {
                    use std::sync::atomic::Ordering as StdOrdering;
                    appended.fetch_max(seq, StdOrdering::SeqCst);
                    let ticket = TestTicket {
                        captured: seq,
                        in_flight: Arc::clone(&in_flight),
                    };
                    let outcome = chain.sync_or_wait_covered(&durable, &ticket, false, || {
                        Ok(TestTicket {
                            captured: appended.load(StdOrdering::SeqCst),
                            in_flight: Arc::clone(&in_flight),
                        })
                    });
                    if let Some(result) = outcome {
                        result.expect("test sync is infallible");
                    }
                    assert!(
                        durable.load(StdOrdering::SeqCst) >= seq,
                        "caller returned with its appends uncovered"
                    );
                })
            })
            .collect();
        for worker in workers {
            worker.join().expect("sync worker completes");
        }
    });
}

/// Shared state for the visibility model: two rows of one batch plus the
/// visible frontier `V`.
#[derive(Debug)]
struct VisibleWorld {
    rows: [AtomicU64; 2],
    visible: AtomicU64,
}

/// Protocol whose apply work writes BOTH rows of batch 2 (the member-side
/// deferred apply of #2682's suspect window).
#[derive(Debug)]
struct VisProtocol;

#[derive(Debug)]
struct VisWork(Arc<VisibleWorld>);

impl GroupProtocol for VisProtocol {
    type Request = u32;
    type Response = u32;
    type Work = VisWork;
    type Done = ();

    fn apply(work: VisWork) {
        // Relaxed on purpose: the protocol's ordering must come from the
        // barrier + Release publish, not from the row stores themselves.
        work.0.rows[0].store(2, Ordering::Relaxed);
        work.0.rows[1].store(2, Ordering::Relaxed);
    }
}

/// A frontier-bounded read: capture `V` (Acquire) first, then read the
/// rows — the product's `load_published_snapshot` discipline.
fn frontier_bounded_read(world: &VisibleWorld) {
    let visible = world.visible.load(Ordering::Acquire);
    if visible >= 2 {
        let row0 = world.rows[0].load(Ordering::Relaxed);
        let row1 = world.rows[1].load(Ordering::Relaxed);
        assert!(
            row0 >= 2 && row1 >= 2,
            "torn read: V={visible} but batch rows read as ({row0}, {row1})"
        );
    }
}

/// The #2682 protocol shape, explored exhaustively: a member thread applies
/// batch 2's rows via the real exchange handoff, the leader publishes the
/// frontier only AFTER the barrier, and a concurrent reader bounds by the
/// frontier. No schedule may show the reader a torn batch.
#[test]
fn loom_visibility_publish_after_barrier_forbids_torn_reads() {
    loom::model(|| {
        let world = Arc::new(VisibleWorld {
            rows: [AtomicU64::new(1), AtomicU64::new(1)],
            visible: AtomicU64::new(1),
        });
        let queue = Arc::new(GroupQueue::<VisProtocol>::default());

        let GroupJoinPath::Lead(_own) = queue.join(0) else {
            panic!("first joiner must lead");
        };
        let leadership = GroupLeadership::new(&queue);
        let GroupJoinPath::Wait(handle) = queue.join(1) else {
            panic!("member must wait behind the leader");
        };

        let member = {
            let queue = Arc::clone(&queue);
            let handle = handle.clone();
            thread::spawn(move || match queue.await_service(&handle) {
                GroupWaitOutcome::Done(response) => *response,
                outcome => panic!("member expected service, got {outcome:?}"),
            })
        };
        let reader = {
            let world = Arc::clone(&world);
            thread::spawn(move || frontier_bounded_read(&world))
        };

        let members = queue.drain_members(16);
        assert_eq!(members.len(), 1);
        let exchange = ExchangeHandle::<VisProtocol>::default();
        match members[0]
            .0
            .request_apply(VisWork(Arc::clone(&world)), &exchange)
        {
            Ok(()) => {
                // Barrier FIRST: the member's row stores are collected...
                assert_eq!(exchange.wait_for(1).len(), 1, "apply outcome lost");
            }
            Err(work) => {
                // ...or the leader applies them itself in this schedule.
                VisProtocol::apply(work);
            }
        }
        // ...and only then does the frontier publish batch 2.
        world.visible.fetch_max(2, Ordering::Release);

        members[0].0.complete(10);
        drop(leadership);
        member.join().expect("member completes");
        reader.join().expect("reader completes");
    });
}

/// Sabotage twin (non-vacuity): publishing the frontier BEFORE the barrier
/// must let loom find the torn read. If this stops panicking, the oracle
/// above has gone blind.
#[test]
#[should_panic(expected = "torn read")]
fn loom_early_publish_is_caught_as_a_torn_read() {
    loom::model(|| {
        let world = Arc::new(VisibleWorld {
            rows: [AtomicU64::new(1), AtomicU64::new(1)],
            visible: AtomicU64::new(1),
        });

        let applier = {
            let world = Arc::clone(&world);
            thread::spawn(move || VisProtocol::apply(VisWork(world)))
        };
        let reader = {
            let world = Arc::clone(&world);
            thread::spawn(move || frontier_bounded_read(&world))
        };

        // The bug under test: visibility published without waiting for the
        // apply barrier.
        world.visible.fetch_max(2, Ordering::Release);

        applier.join().expect("applier completes");
        reader.join().expect("reader completes");
    });
}
