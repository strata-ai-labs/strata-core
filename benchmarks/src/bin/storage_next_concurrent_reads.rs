//! BS2.4b Tier 2 — concurrent read-throughput A/B (control-first).
//!
//! Load L keys, then K reader threads share one runtime and hammer point reads for a fixed window.
//! On the pre-cutover control (commit `b6dab1bc`) each read takes the runtime lock, so aggregate
//! throughput plateaus (or regresses) as K rises; on HEAD reads are off-lock and should scale.
//! Run control-first via `git worktree` and compare. This is a machine-dependent *direction*
//! measurement — the plan's >=3x@10M headline is an out-of-band, big-machine hypothesis.
//!
//! Usage: `cargo run --release --bin storage-next-concurrent-reads`
//!
//! Measured (100K keys, ephemeral cache, this dev box), ops/s:
//!   threads   control b6dab1bc (locked)   HEAD (off-lock)   speedup
//!   1         1_939_900                    1_868_206         0.96x  (single-thread pin cost)
//!   2         1_631_615                    3_308_244         2.0x
//!   4         1_331_889                    5_834_163         4.4x
//!   8           791_807                    8_713_633        11.0x
//! The locked control *regresses* as threads rise (a lock convoy); off-lock scales ~4.7x across
//! 1->8 threads. Consistent with the plan's hypothesis; the >=3x band is met at >=4 threads here.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use strata_storage_next::api::{
    BranchId, CommitBatch, CommitMutation, CommitOptions, PointReadRequest, ReadBound, StorageKey,
    StorageRuntime, StorageSpaceId, StorageValue,
};

const KEYS: usize = 100_000;
const LOAD_BATCH: usize = 1_000;
const READ_WINDOW: Duration = Duration::from_secs(3);
const THREAD_COUNTS: &[usize] = &[1, 2, 4, 8];

fn space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("engine space")
}

fn storage_key(index: usize) -> StorageKey {
    StorageKey::new(format!("k{index:08}").into_bytes()).expect("valid key")
}

fn branch() -> BranchId {
    BranchId::from_bytes([0x01; BranchId::BYTE_LEN])
}

fn main() {
    let mut runtime = StorageRuntime::open_ephemeral()
        .expect("open ephemeral runtime")
        .into_runtime();

    // Load phase (single-threaded): L keys in batches.
    let load_start = Instant::now();
    let mut index = 0;
    while index < KEYS {
        let end = (index + LOAD_BATCH).min(KEYS);
        let mutations: Vec<CommitMutation> = (index..end)
            .map(|i| CommitMutation::Put {
                storage_space: space(),
                key: storage_key(i),
                value: StorageValue::new(vec![0x5A; 32]),
                ttl: None,
            })
            .collect();
        let batch = CommitBatch::new(
            branch(),
            mutations,
            CommitOptions::default().require_conflict_check(false),
        )
        .expect("valid load batch");
        runtime.commit(&batch).expect("load commit");
        index = end;
    }
    eprintln!(
        "loaded {KEYS} keys in {:.2}s",
        load_start.elapsed().as_secs_f64()
    );

    // Pre-build the request set so the read loop measures the read path, not key construction.
    let requests: Vec<PointReadRequest> = (0..KEYS)
        .map(|i| PointReadRequest::new(branch(), space(), storage_key(i), ReadBound::Latest))
        .collect();

    println!("threads,total_ops,ops_per_sec");
    for &threads in THREAD_COUNTS {
        let stop = AtomicBool::new(false);
        let total = AtomicU64::new(0);
        let started = Instant::now();
        std::thread::scope(|scope| {
            for t in 0..threads {
                let (runtime, requests, stop, total) = (&runtime, &requests, &stop, &total);
                scope.spawn(move || {
                    let mut count = 0u64;
                    let mut idx = t * 7_919; // coprime-ish per-thread offset
                    while !stop.load(Ordering::Relaxed) {
                        let request = &requests[idx % KEYS];
                        let outcome = runtime.read_point(request).expect("read");
                        std::hint::black_box(outcome.row().is_some());
                        count += 1;
                        idx += 1;
                    }
                    total.fetch_add(count, Ordering::Relaxed);
                });
            }
            std::thread::sleep(READ_WINDOW);
            stop.store(true, Ordering::Relaxed);
        });
        let elapsed = started.elapsed().as_secs_f64();
        let ops = total.load(Ordering::Relaxed);
        println!("{threads},{ops},{:.0}", ops as f64 / elapsed);
    }
}
