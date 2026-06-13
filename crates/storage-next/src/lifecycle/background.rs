//! Background maintenance scheduler.
//!
//! This module is a storage-next port of `crates/engine/src/background.rs`.
//! It keeps the old scheduler's core semantics: priority ordering, FIFO
//! ordering within a priority, bounded queue backpressure, drain, idempotent
//! shutdown, task-panic containment, and the submit/shutdown race fix.

use parking_lot::{Condvar, Mutex as ParkingMutex};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

/// Priority levels for background lifecycle work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum BackgroundTaskPriority {
    /// Deferred health, retention, purge, and repair work.
    Low = 0,
    /// Compaction and materialization table rewrites.
    Normal = 1,
    /// Flushes, checkpoints, and pressure-clearing work.
    High = 2,
}

/// Error returned when background work cannot be accepted.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct BackgroundBackpressureError;

impl std::fmt::Display for BackgroundBackpressureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("background maintenance scheduler queue is full")
    }
}

impl std::error::Error for BackgroundBackpressureError {}

/// Scheduler metrics snapshot.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct BackgroundSchedulerStats {
    /// Number of tasks waiting in the background queue.
    pub(crate) queue_depth: usize,
    /// Number of tasks currently being executed by workers.
    pub(crate) active_tasks: usize,
    /// Total number of tasks completed since scheduler creation.
    pub(crate) tasks_completed: u64,
    /// Number of worker threads.
    pub(crate) worker_count: usize,
}

struct TaskEnvelope {
    priority: BackgroundTaskPriority,
    sequence: u64,
    work: Box<dyn FnOnce() + Send>,
}

impl Eq for TaskEnvelope {}

impl PartialEq for TaskEnvelope {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

impl Ord for TaskEnvelope {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority
            .cmp(&other.priority)
            .then(other.sequence.cmp(&self.sequence))
    }
}

impl PartialOrd for TaskEnvelope {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// # Memory Ordering
///
/// The metric counters use mixed ordering depending on their role:
///
/// - `queue_depth` and `active_tasks` use `Release` on writes and `Acquire` on
///   correctness-critical reads because they gate backpressure and drain waits.
///   `stats()` reads them with `Relaxed` because approximate monitoring
///   snapshots are acceptable.
/// - `tasks_completed` is purely observational and uses `Relaxed`.
/// - `active_tasks.fetch_sub` uses `AcqRel` because the previous value decides
///   whether drain waiters should be woken.
/// - `sequence` uses `Relaxed`; fetch-add atomicity is enough for uniqueness.
struct SchedulerInner {
    queue: ParkingMutex<BinaryHeap<TaskEnvelope>>,
    work_ready: Condvar,
    drain_cond: Condvar,
    shutdown: AtomicBool,
    sequence: AtomicU64,
    queue_depth: AtomicUsize,
    active_tasks: AtomicUsize,
    max_queue_depth: usize,
    tasks_completed: AtomicU64,
}

/// Priority scheduler for background lifecycle maintenance work.
///
/// Tasks are executed by a fixed worker pool. Higher-priority tasks run first;
/// tasks with the same priority run in FIFO submission order.
pub(crate) struct BackgroundScheduler {
    inner: Arc<SchedulerInner>,
    workers: ParkingMutex<Vec<JoinHandle<()>>>,
    worker_count: usize,
}

impl std::fmt::Debug for BackgroundScheduler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BackgroundScheduler")
            .field("stats", &self.stats())
            .finish()
    }
}

impl BackgroundScheduler {
    /// Creates a scheduler using the default storage-next maintenance thread
    /// name prefix.
    pub(crate) fn new(worker_count: usize, max_queue_depth: usize) -> Self {
        Self::with_thread_name_prefix(worker_count, max_queue_depth, "strata-storage-maint-bg")
    }

    /// Creates a scheduler using a caller-provided thread name prefix.
    pub(crate) fn with_thread_name_prefix(
        worker_count: usize,
        max_queue_depth: usize,
        thread_name_prefix: impl Into<String>,
    ) -> Self {
        assert!(
            worker_count > 0,
            "background maintenance scheduler requires at least one worker thread"
        );
        let inner = Arc::new(SchedulerInner {
            queue: ParkingMutex::new(BinaryHeap::new()),
            work_ready: Condvar::new(),
            drain_cond: Condvar::new(),
            shutdown: AtomicBool::new(false),
            sequence: AtomicU64::new(0),
            queue_depth: AtomicUsize::new(0),
            active_tasks: AtomicUsize::new(0),
            max_queue_depth,
            tasks_completed: AtomicU64::new(0),
        });

        let thread_name_prefix = thread_name_prefix.into();
        let mut workers = Vec::with_capacity(worker_count);
        for index in 0..worker_count {
            let inner = Arc::clone(&inner);
            let thread_name = format!("{thread_name_prefix}-{index}");
            let handle = std::thread::Builder::new()
                .name(thread_name)
                .spawn(move || worker_loop(&inner))
                .expect("failed to spawn background maintenance worker thread");
            workers.push(handle);
        }

        Self {
            inner,
            workers: ParkingMutex::new(workers),
            worker_count,
        }
    }

    /// Submits a task to the background scheduler.
    ///
    /// Returns `Err` when the queue is at capacity or the scheduler has shut
    /// down.
    pub(crate) fn submit(
        &self,
        priority: BackgroundTaskPriority,
        work: impl FnOnce() + Send + 'static,
    ) -> Result<(), BackgroundBackpressureError> {
        if self.inner.shutdown.load(AtomicOrdering::Acquire) {
            return Err(BackgroundBackpressureError);
        }

        if self.inner.queue_depth.load(AtomicOrdering::Acquire) >= self.inner.max_queue_depth {
            return Err(BackgroundBackpressureError);
        }

        let sequence = self.inner.sequence.fetch_add(1, AtomicOrdering::Relaxed);
        let envelope = TaskEnvelope {
            priority,
            sequence,
            work: Box::new(work),
        };

        {
            let mut queue = self.inner.queue.lock();
            // Authoritative shutdown check under lock. `shutdown()` takes this
            // same lock before notifying and joining, so accepted work cannot
            // land in a dead queue.
            if self.inner.shutdown.load(AtomicOrdering::Acquire) {
                return Err(BackgroundBackpressureError);
            }
            // Storage-next preserves the old scheduler's early backpressure
            // check and adds this lock-held recheck so concurrent submitters
            // cannot push the queue past its configured depth.
            if self.inner.queue_depth.load(AtomicOrdering::Acquire) >= self.inner.max_queue_depth {
                return Err(BackgroundBackpressureError);
            }
            queue.push(envelope);
            self.inner.queue_depth.fetch_add(1, AtomicOrdering::Release);
        }

        self.inner.work_ready.notify_one();
        Ok(())
    }

    /// Blocks until all queued and in-flight tasks have completed.
    ///
    /// Workers remain alive after drain returns.
    pub(crate) fn drain(&self) {
        let mut queue = self.inner.queue.lock();
        while self.inner.queue_depth.load(AtomicOrdering::Acquire) > 0
            || self.inner.active_tasks.load(AtomicOrdering::Acquire) > 0
        {
            self.inner.drain_cond.wait(&mut queue);
        }
    }

    /// Blocks until at least one accepted task completes, the scheduler becomes
    /// idle, or `deadline` is reached.
    pub(crate) fn wait_for_progress_until(
        &self,
        completed_before_wait: u64,
        deadline: Instant,
    ) -> bool {
        let mut queue = self.inner.queue.lock();
        loop {
            if self.inner.tasks_completed.load(AtomicOrdering::Acquire) > completed_before_wait {
                return true;
            }
            if self.inner.queue_depth.load(AtomicOrdering::Acquire) == 0
                && self.inner.active_tasks.load(AtomicOrdering::Acquire) == 0
            {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let timeout = deadline.saturating_duration_since(now);
            if self
                .inner
                .drain_cond
                .wait_for(&mut queue, timeout)
                .timed_out()
            {
                return self.inner.tasks_completed.load(AtomicOrdering::Acquire)
                    > completed_before_wait
                    || (self.inner.queue_depth.load(AtomicOrdering::Acquire) == 0
                        && self.inner.active_tasks.load(AtomicOrdering::Acquire) == 0);
            }
        }
    }

    /// Signals workers to drain accepted work and exit.
    ///
    /// Repeated shutdown calls are allowed.
    pub(crate) fn shutdown(&self) {
        self.inner.shutdown.store(true, AtomicOrdering::Release);

        {
            let _queue = self.inner.queue.lock();
            self.inner.work_ready.notify_all();
        }

        let (current_worker, workers_to_join) = {
            let current_thread_id = std::thread::current().id();
            let mut current_worker = false;
            let mut current_worker_handle = None;
            let mut workers_to_join = Vec::new();
            let mut workers = self.workers.lock();
            for handle in workers.drain(..) {
                if handle.thread().id() == current_thread_id {
                    current_worker = true;
                    current_worker_handle = Some(handle);
                } else {
                    workers_to_join.push(handle);
                }
            }
            if let Some(handle) = current_worker_handle {
                workers.push(handle);
            }
            (current_worker, workers_to_join)
        };

        for handle in workers_to_join {
            let _ = handle.join();
        }

        if current_worker {
            drain_ready_tasks_on_current_thread(&self.inner);
        }
    }

    /// Returns an observational metrics snapshot.
    pub(crate) fn stats(&self) -> BackgroundSchedulerStats {
        BackgroundSchedulerStats {
            queue_depth: self.inner.queue_depth.load(AtomicOrdering::Relaxed),
            active_tasks: self.inner.active_tasks.load(AtomicOrdering::Relaxed),
            tasks_completed: self.inner.tasks_completed.load(AtomicOrdering::Relaxed),
            worker_count: self.worker_count,
        }
    }
}

struct ActiveTaskGuard<'a> {
    inner: &'a SchedulerInner,
}

impl Drop for ActiveTaskGuard<'_> {
    fn drop(&mut self) {
        self.inner.active_tasks.fetch_sub(1, AtomicOrdering::AcqRel);
        self.inner
            .tasks_completed
            .fetch_add(1, AtomicOrdering::Relaxed);

        let _queue = self.inner.queue.lock();
        self.inner.drain_cond.notify_all();
    }
}

fn worker_loop(inner: &SchedulerInner) {
    loop {
        let task = {
            let mut queue = inner.queue.lock();
            loop {
                if let Some(task) = queue.pop() {
                    inner.queue_depth.fetch_sub(1, AtomicOrdering::Release);
                    inner.active_tasks.fetch_add(1, AtomicOrdering::Release);
                    break task;
                }
                if inner.shutdown.load(AtomicOrdering::Acquire) {
                    return;
                }
                inner.work_ready.wait(&mut queue);
            }
        };

        let _guard = ActiveTaskGuard { inner };
        if let Err(error) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task.work)) {
            tracing::error!(
                "background maintenance task panicked: {:?}",
                error
                    .downcast_ref::<&str>()
                    .copied()
                    .unwrap_or("(non-string panic)")
            );
        }
    }
}

fn drain_ready_tasks_on_current_thread(inner: &SchedulerInner) {
    loop {
        let task = {
            let mut queue = inner.queue.lock();
            queue.pop().inspect(|_| {
                inner.queue_depth.fetch_sub(1, AtomicOrdering::Release);
                inner.active_tasks.fetch_add(1, AtomicOrdering::Release);
            })
        };

        let Some(task) = task else {
            return;
        };

        let _guard = ActiveTaskGuard { inner };
        if let Err(error) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task.work)) {
            tracing::error!(
                "background maintenance task panicked: {:?}",
                error
                    .downcast_ref::<&str>()
                    .copied()
                    .unwrap_or("(non-string panic)")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;
    use std::sync::{Arc, Barrier};

    #[test]
    fn background_submit_and_drain() {
        let scheduler = BackgroundScheduler::new(2, 4096);
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..10 {
            let counter = Arc::clone(&counter);
            scheduler
                .submit(BackgroundTaskPriority::Normal, move || {
                    counter.fetch_add(1, AtomicOrdering::Relaxed);
                })
                .expect("submit task");
        }

        scheduler.drain();
        assert_eq!(counter.load(AtomicOrdering::Relaxed), 10);
        scheduler.shutdown();
    }

    #[test]
    fn background_priority_ordering() {
        let scheduler = BackgroundScheduler::new(1, 4096);
        let barrier = Arc::new(Barrier::new(2));
        let task_barrier = Arc::clone(&barrier);
        scheduler
            .submit(BackgroundTaskPriority::Low, move || {
                task_barrier.wait();
            })
            .expect("submit barrier");

        std::thread::sleep(std::time::Duration::from_millis(50));

        let order = Arc::new(ParkingMutex::new(Vec::new()));

        let observed = Arc::clone(&order);
        scheduler
            .submit(BackgroundTaskPriority::Low, move || {
                observed.lock().push("low");
            })
            .expect("submit low");

        let observed = Arc::clone(&order);
        scheduler
            .submit(BackgroundTaskPriority::Normal, move || {
                observed.lock().push("normal");
            })
            .expect("submit normal");

        let observed = Arc::clone(&order);
        scheduler
            .submit(BackgroundTaskPriority::High, move || {
                observed.lock().push("high");
            })
            .expect("submit high");

        barrier.wait();
        scheduler.drain();

        assert_eq!(order.lock().clone(), vec!["high", "normal", "low"]);
        scheduler.shutdown();
    }

    #[test]
    fn background_fifo_within_same_priority() {
        let scheduler = BackgroundScheduler::new(1, 4096);
        let barrier = Arc::new(Barrier::new(2));
        let task_barrier = Arc::clone(&barrier);
        scheduler
            .submit(BackgroundTaskPriority::Low, move || {
                task_barrier.wait();
            })
            .expect("submit barrier");

        std::thread::sleep(std::time::Duration::from_millis(50));

        let order = Arc::new(ParkingMutex::new(Vec::new()));
        for value in 0..5 {
            let observed = Arc::clone(&order);
            scheduler
                .submit(BackgroundTaskPriority::Normal, move || {
                    observed.lock().push(value);
                })
                .expect("submit normal");
        }

        barrier.wait();
        scheduler.drain();

        assert_eq!(order.lock().clone(), vec![0, 1, 2, 3, 4]);
        scheduler.shutdown();
    }

    #[test]
    fn background_backpressure() {
        let scheduler = BackgroundScheduler::new(1, 2);
        let barrier = Arc::new(Barrier::new(2));
        let task_barrier = Arc::clone(&barrier);
        scheduler
            .submit(BackgroundTaskPriority::Normal, move || {
                task_barrier.wait();
            })
            .expect("submit barrier");

        std::thread::sleep(std::time::Duration::from_millis(50));

        let counter = Arc::new(AtomicUsize::new(0));
        for _ in 0..2 {
            let counter = Arc::clone(&counter);
            scheduler
                .submit(BackgroundTaskPriority::Normal, move || {
                    counter.fetch_add(1, AtomicOrdering::Relaxed);
                })
                .expect("submit queued task");
        }

        assert!(scheduler
            .submit(BackgroundTaskPriority::Normal, || {})
            .is_err());

        barrier.wait();
        scheduler.drain();
        assert_eq!(counter.load(AtomicOrdering::Relaxed), 2);
        scheduler.shutdown();
    }

    #[test]
    fn background_concurrent_backpressure_does_not_exceed_queue_depth() {
        for _ in 0..25 {
            let scheduler = Arc::new(BackgroundScheduler::new(1, 1));
            let worker_barrier = Arc::new(Barrier::new(2));
            let task_barrier = Arc::clone(&worker_barrier);
            scheduler
                .submit(BackgroundTaskPriority::Normal, move || {
                    task_barrier.wait();
                })
                .expect("submit barrier");

            std::thread::sleep(std::time::Duration::from_millis(50));

            let submitter_count = 16;
            let start = Arc::new(Barrier::new(submitter_count));
            let accepted = Arc::new(AtomicUsize::new(0));
            let mut handles = Vec::new();
            for _ in 0..submitter_count {
                let scheduler = Arc::clone(&scheduler);
                let start = Arc::clone(&start);
                let accepted = Arc::clone(&accepted);
                handles.push(std::thread::spawn(move || {
                    start.wait();
                    if scheduler
                        .submit(BackgroundTaskPriority::Normal, || {})
                        .is_ok()
                    {
                        accepted.fetch_add(1, AtomicOrdering::Relaxed);
                    }
                }));
            }

            for handle in handles {
                handle.join().expect("submitter joined");
            }

            assert!(
                accepted.load(AtomicOrdering::Relaxed) <= 1,
                "accepted more queued tasks than max queue depth"
            );
            worker_barrier.wait();
            scheduler.shutdown();
        }
    }

    #[test]
    #[should_panic(
        expected = "background maintenance scheduler requires at least one worker thread"
    )]
    fn background_zero_workers_rejected() {
        let _scheduler = BackgroundScheduler::new(0, 1);
    }

    #[test]
    fn background_shutdown_drains_remaining() {
        let scheduler = BackgroundScheduler::new(1, 4096);
        let barrier = Arc::new(Barrier::new(2));
        let task_barrier = Arc::clone(&barrier);
        scheduler
            .submit(BackgroundTaskPriority::Normal, move || {
                task_barrier.wait();
            })
            .expect("submit barrier");

        std::thread::sleep(std::time::Duration::from_millis(50));

        let counter = Arc::new(AtomicUsize::new(0));
        for _ in 0..5 {
            let counter = Arc::clone(&counter);
            scheduler
                .submit(BackgroundTaskPriority::Normal, move || {
                    counter.fetch_add(1, AtomicOrdering::Relaxed);
                })
                .expect("submit queued task");
        }

        barrier.wait();
        scheduler.shutdown();

        assert_eq!(counter.load(AtomicOrdering::Relaxed), 5);
    }

    #[test]
    fn background_drain_returns_when_idle() {
        let scheduler = BackgroundScheduler::new(2, 4096);
        scheduler.drain();
        scheduler.shutdown();
    }

    #[test]
    fn background_stats() {
        let scheduler = BackgroundScheduler::new(2, 4096);
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..5 {
            let counter = Arc::clone(&counter);
            scheduler
                .submit(BackgroundTaskPriority::Normal, move || {
                    counter.fetch_add(1, AtomicOrdering::Relaxed);
                })
                .expect("submit task");
        }

        scheduler.drain();

        let stats = scheduler.stats();
        assert_eq!(stats.tasks_completed, 5);
        assert_eq!(stats.queue_depth, 0);
        assert_eq!(stats.active_tasks, 0);
        assert_eq!(stats.worker_count, 2);
        scheduler.shutdown();
    }

    #[test]
    fn background_wait_for_progress_observes_task_completion() {
        let scheduler = BackgroundScheduler::new(1, 4096);
        let completed_before_wait = scheduler.stats().tasks_completed;
        let counter = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&counter);
        scheduler
            .submit(BackgroundTaskPriority::Normal, move || {
                observed.fetch_add(1, AtomicOrdering::Relaxed);
            })
            .expect("submit task");

        assert!(scheduler.wait_for_progress_until(
            completed_before_wait,
            Instant::now() + std::time::Duration::from_secs(5)
        ));
        assert_eq!(counter.load(AtomicOrdering::Relaxed), 1);
        scheduler.shutdown();
    }

    #[test]
    fn background_wait_for_progress_times_out_without_progress() {
        let scheduler = BackgroundScheduler::new(1, 4096);
        let worker_started = Arc::new(Barrier::new(2));
        let worker_release = Arc::new(Barrier::new(2));
        let started = Arc::clone(&worker_started);
        let release = Arc::clone(&worker_release);
        scheduler
            .submit(BackgroundTaskPriority::Normal, move || {
                started.wait();
                release.wait();
            })
            .expect("submit barrier task");
        worker_started.wait();

        let completed_before_wait = scheduler.stats().tasks_completed;
        assert!(!scheduler.wait_for_progress_until(
            completed_before_wait,
            Instant::now() + std::time::Duration::from_millis(25)
        ));

        worker_release.wait();
        scheduler.drain();
        scheduler.shutdown();
    }

    #[test]
    fn background_stats_report_active_and_queued_tasks_before_drain() {
        let scheduler = BackgroundScheduler::new(1, 4096);
        let worker_started = Arc::new(Barrier::new(2));
        let worker_release = Arc::new(Barrier::new(2));
        let started = Arc::clone(&worker_started);
        let release = Arc::clone(&worker_release);
        scheduler
            .submit(BackgroundTaskPriority::Normal, move || {
                started.wait();
                release.wait();
            })
            .expect("submit barrier");

        worker_started.wait();
        let active_stats = scheduler.stats();
        assert_eq!(active_stats.active_tasks, 1);
        assert_eq!(active_stats.queue_depth, 0);

        scheduler
            .submit(BackgroundTaskPriority::Normal, || {})
            .expect("submit queued one");
        scheduler
            .submit(BackgroundTaskPriority::Normal, || {})
            .expect("submit queued two");

        let queued_stats = scheduler.stats();
        assert_eq!(queued_stats.active_tasks, 1);
        assert_eq!(queued_stats.queue_depth, 2);

        worker_release.wait();
        scheduler.drain();
        let drained_stats = scheduler.stats();
        assert_eq!(drained_stats.active_tasks, 0);
        assert_eq!(drained_stats.queue_depth, 0);
        assert_eq!(drained_stats.tasks_completed, 3);
        scheduler.shutdown();
    }

    #[test]
    fn background_default_worker_thread_uses_storage_next_prefix() {
        let scheduler = BackgroundScheduler::new(1, 4096);
        let (sender, receiver) = mpsc::channel();

        scheduler
            .submit(BackgroundTaskPriority::Normal, move || {
                let name = std::thread::current().name().map(str::to_owned);
                sender.send(name).expect("send thread name");
            })
            .expect("submit thread-name task");
        scheduler.drain();

        let name = receiver
            .recv()
            .expect("receive thread name")
            .expect("worker thread is named");
        assert!(
            name.starts_with("strata-storage-maint-bg-"),
            "unexpected background worker thread name: {name}"
        );
        scheduler.shutdown();
    }

    #[test]
    fn background_submit_after_shutdown_rejected() {
        let scheduler = BackgroundScheduler::new(2, 4096);
        scheduler.shutdown();

        let result = scheduler.submit(BackgroundTaskPriority::Normal, || {});
        assert!(result.is_err());
    }

    #[test]
    fn background_task_panic_does_not_hang_drain() {
        let scheduler = BackgroundScheduler::new(2, 4096);
        let counter = Arc::new(AtomicUsize::new(0));

        scheduler
            .submit(BackgroundTaskPriority::Normal, || {
                panic!("intentional background test panic");
            })
            .expect("submit panic task");

        for _ in 0..5 {
            let counter = Arc::clone(&counter);
            scheduler
                .submit(BackgroundTaskPriority::Normal, move || {
                    counter.fetch_add(1, AtomicOrdering::Relaxed);
                })
                .expect("submit normal task");
        }

        scheduler.drain();

        assert_eq!(counter.load(AtomicOrdering::Relaxed), 5);
        assert_eq!(scheduler.stats().tasks_completed, 6);
        scheduler.shutdown();
    }

    #[test]
    fn background_concurrent_submits() {
        let scheduler = Arc::new(BackgroundScheduler::new(2, 4096));
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..4 {
            let scheduler = Arc::clone(&scheduler);
            let counter = Arc::clone(&counter);
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    let counter = Arc::clone(&counter);
                    scheduler
                        .submit(BackgroundTaskPriority::Normal, move || {
                            counter.fetch_add(1, AtomicOrdering::Relaxed);
                        })
                        .expect("submit concurrent task");
                }
            }));
        }

        for handle in handles {
            handle.join().expect("submitter joined");
        }

        scheduler.drain();
        assert_eq!(counter.load(AtomicOrdering::Relaxed), 400);
        assert_eq!(scheduler.stats().tasks_completed, 400);
        scheduler.shutdown();
    }

    #[test]
    fn background_shutdown_is_idempotent() {
        let scheduler = BackgroundScheduler::new(2, 4096);
        scheduler
            .submit(BackgroundTaskPriority::Normal, || {})
            .expect("submit task");
        scheduler.drain();

        scheduler.shutdown();
        scheduler.shutdown();
        scheduler.shutdown();
    }

    #[test]
    fn background_shutdown_from_worker_does_not_join_itself() {
        let scheduler = Arc::new(BackgroundScheduler::new(1, 4096));
        let task_started = Arc::new(Barrier::new(2));
        let release_task = Arc::new(Barrier::new(2));
        let (sender, receiver) = mpsc::channel();
        let trailing_counter = Arc::new(AtomicUsize::new(0));

        let task_scheduler = Arc::clone(&scheduler);
        let started = Arc::clone(&task_started);
        let release = Arc::clone(&release_task);
        let observed = Arc::clone(&trailing_counter);
        scheduler
            .submit(BackgroundTaskPriority::Normal, move || {
                started.wait();
                release.wait();
                task_scheduler.shutdown();
                sender
                    .send(observed.load(AtomicOrdering::Relaxed))
                    .expect("send shutdown completion");
            })
            .expect("submit shutdown task");

        task_started.wait();
        for _ in 0..2 {
            let observed = Arc::clone(&trailing_counter);
            scheduler
                .submit(BackgroundTaskPriority::Normal, move || {
                    observed.fetch_add(1, AtomicOrdering::Relaxed);
                })
                .expect("submit trailing task");
        }
        release_task.wait();

        assert_eq!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("worker shutdown task completed"),
            2
        );
        assert!(scheduler
            .submit(BackgroundTaskPriority::Normal, || {})
            .is_err());
        scheduler.shutdown();
    }

    #[test]
    fn background_shutdown_from_worker_contains_trailing_task_panic() {
        let scheduler = Arc::new(BackgroundScheduler::new(1, 4096));
        let task_started = Arc::new(Barrier::new(2));
        let release_task = Arc::new(Barrier::new(2));
        let (sender, receiver) = mpsc::channel();

        let task_scheduler = Arc::clone(&scheduler);
        let started = Arc::clone(&task_started);
        let release = Arc::clone(&release_task);
        scheduler
            .submit(BackgroundTaskPriority::Normal, move || {
                started.wait();
                release.wait();
                task_scheduler.shutdown();
                sender.send(()).expect("send shutdown completion");
            })
            .expect("submit shutdown task");

        task_started.wait();
        scheduler
            .submit(BackgroundTaskPriority::Normal, || {
                panic!("intentional trailing shutdown panic");
            })
            .expect("submit trailing panic task");
        release_task.wait();

        receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("worker shutdown task completed");
        assert!(scheduler
            .submit(BackgroundTaskPriority::Normal, || {})
            .is_err());
        scheduler.shutdown();
    }

    #[test]
    fn background_submit_shutdown_toctou() {
        for _ in 0..50 {
            let scheduler = Arc::new(BackgroundScheduler::new(1, 4096));
            let executed = Arc::new(AtomicUsize::new(0));
            let submitted = Arc::new(AtomicUsize::new(0));
            let barrier = Arc::new(Barrier::new(2));

            let submit_scheduler = Arc::clone(&scheduler);
            let submit_executed = Arc::clone(&executed);
            let submit_count = Arc::clone(&submitted);
            let submit_barrier = Arc::clone(&barrier);
            let submitter = std::thread::spawn(move || {
                submit_barrier.wait();
                for _ in 0..100 {
                    let executed = Arc::clone(&submit_executed);
                    if submit_scheduler
                        .submit(BackgroundTaskPriority::Normal, move || {
                            executed.fetch_add(1, AtomicOrdering::Relaxed);
                        })
                        .is_ok()
                    {
                        submit_count.fetch_add(1, AtomicOrdering::Relaxed);
                    }
                }
            });

            let shutdown_scheduler = Arc::clone(&scheduler);
            let shutdown_barrier = Arc::clone(&barrier);
            let shutdowner = std::thread::spawn(move || {
                shutdown_barrier.wait();
                shutdown_scheduler.shutdown();
            });

            submitter.join().expect("submitter joined");
            shutdowner.join().expect("shutdowner joined");

            let submitted = submitted.load(AtomicOrdering::Relaxed);
            let executed = executed.load(AtomicOrdering::Relaxed);
            assert_eq!(
                executed, submitted,
                "dropped accepted tasks: submitted={submitted}, executed={executed}",
            );
        }
    }

    #[test]
    fn background_drain_then_submit_then_drain() {
        let scheduler = BackgroundScheduler::new(2, 4096);
        let counter = Arc::new(AtomicUsize::new(0));

        let submitted = Arc::clone(&counter);
        scheduler
            .submit(BackgroundTaskPriority::Normal, move || {
                submitted.fetch_add(1, AtomicOrdering::Relaxed);
            })
            .expect("submit first task");
        scheduler.drain();
        assert_eq!(counter.load(AtomicOrdering::Relaxed), 1);

        let submitted = Arc::clone(&counter);
        scheduler
            .submit(BackgroundTaskPriority::Normal, move || {
                submitted.fetch_add(1, AtomicOrdering::Relaxed);
            })
            .expect("submit second task");
        scheduler.drain();
        assert_eq!(counter.load(AtomicOrdering::Relaxed), 2);

        scheduler.shutdown();
    }
}
