//! GT5 microbench harness (design §13): the tier's device operations,
//! measured on real hardware against the acceptance budget.
//!
//! ```bash
//! cargo run -p strata-gpu-cache --release --example microbench
//! ```
//!
//! Per-op latencies are enqueue-to-complete (poll-spun readiness, no host
//! synchronization inside the measured region), averaged over warm
//! iterations. The budget context: selection + expansion + gather must fit
//! in 400 us (20% of a 2 ms step at 500M on the 4070S).

#![allow(
    clippy::cast_precision_loss,
    reason = "benchmark arithmetic: latencies and byte counts printed as floats"
)]

use std::time::Instant;

use strata_gpu_cache::tier::backend::{
    scratch_bytes, CopyFence, DeviceBackend, Region, RegionBytes,
};
use strata_gpu_cache::tier::engine_store::EnginePageStore;
use strata_gpu_cache::tier::store::PageBlob;
use strata_gpu_cache::tier::CudaBackend;

const WARMUP: usize = 10;
const ITERS: u32 = 100;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

fn seeded_backend(capacity: usize, dim: usize, degree: usize, page_bytes: usize) -> CudaBackend {
    let mut backend = CudaBackend::new(page_bytes.max(1 << 16)).expect("device present");
    backend
        .reserve(RegionBytes {
            pages: (capacity * page_bytes) as u64,
            summaries: (capacity * dim * 4) as u64,
            adjacency: (capacity * degree * 4) as u64,
            validity: capacity as u64,
            tags: (capacity * 32) as u64,
            scratch: scratch_bytes(capacity as u64, (dim * 4) as u64),
            materialize: 64 * page_bytes as u64,
        })
        .expect("reserve");
    let mut rng = Lcg(0xB13B);
    // Bulk writes: chunked validity/summaries/adjacency so setup stays fast.
    let validity: Vec<u8> = (0..capacity)
        .map(|_| u8::from(rng.next() % 8 != 0))
        .collect();
    backend
        .copy_in(Region::Validity, 0, &validity)
        .expect("validity");
    let mut summaries = Vec::with_capacity(capacity * dim * 4);
    for _ in 0..capacity * dim {
        #[allow(clippy::cast_precision_loss)]
        let v = (rng.next() % 16) as f32 - 8.0;
        summaries.extend_from_slice(&v.to_le_bytes());
    }
    for (index, chunk) in summaries.chunks(1 << 16).enumerate() {
        backend
            .copy_in(Region::Summaries, (index * (1 << 16)) as u64, chunk)
            .expect("summaries");
    }
    let mut adjacency = Vec::with_capacity(capacity * degree * 4);
    for _ in 0..capacity * degree {
        let entry = if rng.next() % 3 == 0 {
            u32::MAX
        } else {
            u32::try_from(rng.next() % capacity as u64).unwrap()
        };
        adjacency.extend_from_slice(&entry.to_le_bytes());
    }
    for (index, chunk) in adjacency.chunks(1 << 16).enumerate() {
        backend
            .copy_in(Region::Adjacency, (index * (1 << 16)) as u64, chunk)
            .expect("adjacency");
    }
    backend
}

/// Enqueue-to-complete latency of one op, poll-spun, averaged.
fn time_op(mut op: impl FnMut() -> Box<dyn Fn() -> bool>) -> f64 {
    for _ in 0..WARMUP {
        let ready = op();
        while !ready() {
            std::hint::spin_loop();
        }
    }
    let start = Instant::now();
    for _ in 0..ITERS {
        let ready = op();
        while !ready() {
            std::hint::spin_loop();
        }
    }
    start.elapsed().as_secs_f64() * 1e6 / f64::from(ITERS)
}

fn bench_selection(capacity: usize, dim: usize, degree: usize) {
    let mut backend = seeded_backend(capacity, dim, degree, 256);
    let query: Vec<f32> = (0..dim).map(|i| (i % 5) as f32 - 2.0).collect();
    for k in [8u16, 32, 64] {
        let micros = time_op(|| {
            let fence = backend.topk(&query, k, None, None).expect("topk");
            Box::new(move || fence.is_complete())
        });
        println!("topk           capacity={capacity:>6} k={k:<3}            {micros:>9.1} us");
    }
    let micros = time_op(|| {
        let fence = backend
            .topk(&query, 64, Some(256), None)
            .expect("topk+expand");
        Box::new(move || fence.is_complete())
    });
    println!("topk+expand    capacity={capacity:>6} k=64  F={degree:<3}     {micros:>9.1} us");
}

fn bench_materialize(page_bytes: usize) {
    let capacity = 4096;
    let mut backend = seeded_backend(capacity, 16, 8, page_bytes);
    let query: Vec<f32> = (0..16).map(|i| (i % 5) as f32 - 2.0).collect();
    let fence = backend.topk(&query, 64, None, None).expect("topk");
    while !fence.is_complete() {
        std::hint::spin_loop();
    }
    let micros = time_op(|| {
        let fence = backend.materialize_topk().expect("materialize");
        Box::new(move || fence.is_complete())
    });
    let bytes = 64.0 * page_bytes as f64;
    let gbps = bytes / (micros * 1e-6) / 1e9;
    println!(
        "materialize    k=64 page={page_bytes:>6}B          {micros:>9.1} us   {gbps:>6.1} GB/s"
    );
}

fn bench_promotion_and_append() {
    use strata_gpu_cache::tier::page_table::PageId;
    use strata_gpu_cache::tier::store::InMemoryStore;
    use strata_gpu_cache::tier::tier::{Tier, TierConfig};

    const PAGE: usize = 64 << 10;
    const SLOTS: u32 = 64;
    const APPENDS: usize = 64;
    let config = TierConfig {
        page_bytes: PAGE as u64,
        summary_bytes: 64,
        page_slots: SLOTS,
        promotion_batch: 16,
        adjacency_degree: 32,
        write_behind_batch: 16,
        write_backlog_cap: 64,
    };

    // Promotion throughput: cold pages through the staging pipeline.
    let mut store = InMemoryStore::new();
    let blob = PageBlob {
        bytes: vec![7u8; PAGE],
        summary: vec![1u8; 64],
        tags: [0; 4],
        edges: Vec::new(),
    };
    for id in 0..256u64 {
        store.seed(PageId(id), blob.clone());
    }
    let backend = CudaBackend::new(PAGE).expect("device");
    let mut tier = Tier::open(backend, store, config).expect("tier");
    let start = Instant::now();
    let mut promoted = 0u64;
    // Batched windows over a working set 4x the pool, so evictions must
    // run. Re-requesting every round is the documented caller contract:
    // requests dedup while queued and re-queue degraded placements.
    'windows: for window in 0..16u64 {
        let batch: Vec<PageId> = (window * 16..window * 16 + 16).map(PageId).collect();
        while !batch.iter().all(|id| tier.is_selectable(*id)) {
            for id in &batch {
                tier.request(*id, 1);
            }
            tier.step_begin().expect("step");
            tier.maintain();
            if start.elapsed().as_secs() > 30 {
                break 'windows;
            }
        }
        promoted += 16;
    }
    let secs = start.elapsed().as_secs_f64();
    let mbps = promoted as f64 * PAGE as f64 / secs / 1e6;
    println!(
        "promotion      {promoted:>4} x 64KiB pages          {:>9.1} ms  {mbps:>6.1} MB/s",
        secs * 1e3
    );

    // Append -> durable: engine-backed, batched commits.
    let dir = tempfile::tempdir().expect("tempdir");
    let store = EnginePageStore::open(dir.path(), "bench").expect("store");
    let backend = CudaBackend::new(PAGE).expect("device");
    let mut tier = Tier::open(backend, store, config).expect("tier");
    let mut worst = 0.0f64;
    let mut total = 0.0f64;
    for _ in 0..APPENDS {
        let start = Instant::now();
        tier.append(&blob).expect("append");
        tier.maintain(); // batches of 16 commit inline
        let micros = start.elapsed().as_secs_f64() * 1e6;
        total += micros;
        worst = worst.max(micros);
    }
    let flush_start = Instant::now();
    tier.flush().expect("flush");
    let flush_micros = flush_start.elapsed().as_secs_f64() * 1e6;
    println!(
        "append         mean={:>7.1} us  worst={worst:>9.1} us  final flush={flush_micros:>9.1} us",
        total / APPENDS as f64
    );
}

fn main() {
    println!("== gpu-cache microbenches (4070S; budget: select+expand+gather <= 400 us) ==");
    for capacity in [1usize << 10, 8 << 10, 64 << 10] {
        bench_selection(capacity, 16, 32);
    }
    bench_materialize(64 << 10);
    bench_promotion_and_append();
}
