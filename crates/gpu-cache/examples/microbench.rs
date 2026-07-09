//! GT5 microbench harness (design §13): the tier's device operations,
//! measured on real hardware against the acceptance budget — and the
//! benchmarking endpoint for replacement selection modules (design D3).
//!
//! ```bash
//! # Baseline
//! cargo run -p strata-gpu-cache --release --example microbench
//!
//! # Your module instead of the baseline
//! cargo run -p strata-gpu-cache --release --example microbench -- --ptx moho.ptx
//!
//! # Baseline vs your module, identical seeded state, with deltas
//! cargo run -p strata-gpu-cache --release --example microbench -- --ptx moho.ptx --compare
//!
//! # Machine-readable (for tracking in CI)
//! cargo run -p strata-gpu-cache --release --example microbench -- --json
//! ```
//!
//! Per-op latencies are enqueue-to-complete (poll-spun readiness, no host
//! synchronization inside the measured region), averaged over warm
//! iterations. The `stage:*` rows are pure device time from CUDA event
//! timestamps (profiling mode), free of host enqueue overhead — the numbers
//! to compare kernel against kernel. The budget context: selection +
//! expansion + gather must fit in 400 us (20% of a 2 ms step at 500M on the
//! 4070S).

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
const STAGE_ITERS: u32 = 50;

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

/// One measurement. Labels are stable identifiers: `--compare` joins the
/// baseline and custom runs on them, and `--json` emits them verbatim.
struct Row {
    label: String,
    micros: f64,
    note: String,
}

impl Row {
    fn new(label: impl Into<String>, micros: f64, note: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            micros,
            note: note.into(),
        }
    }
}

fn seeded_backend(
    capacity: usize,
    dim: usize,
    degree: usize,
    page_bytes: usize,
    ptx: Option<&str>,
) -> CudaBackend {
    let mut backend = CudaBackend::new(page_bytes.max(1 << 16)).expect("device present");
    if let Some(source) = ptx {
        backend
            .register_selection_ptx(source)
            .expect("register replacement module");
    }
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

fn bench_selection(capacity: usize, dim: usize, degree: usize, ptx: Option<&str>) -> Vec<Row> {
    let mut backend = seeded_backend(capacity, dim, degree, 256, ptx);
    let query: Vec<f32> = (0..dim).map(|i| (i % 5) as f32 - 2.0).collect();
    let mut rows = Vec::new();
    for k in [8u16, 32, 64] {
        let micros = time_op(|| {
            let fence = backend.topk(&query, k, None, None).expect("topk");
            Box::new(move || fence.is_complete())
        });
        rows.push(Row::new(format!("topk cap={capacity} k={k}"), micros, ""));
    }
    let micros = time_op(|| {
        let fence = backend
            .topk(&query, 64, Some(256), None)
            .expect("topk+expand");
        Box::new(move || fence.is_complete())
    });
    rows.push(Row::new(
        format!("topk+expand cap={capacity} k=64 F={degree}"),
        micros,
        "",
    ));
    rows
}

fn bench_materialize(page_bytes: usize, ptx: Option<&str>) -> Row {
    let capacity = 4096;
    let mut backend = seeded_backend(capacity, 16, 8, page_bytes, ptx);
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
    Row::new(
        format!("materialize k=64 page={page_bytes}B"),
        micros,
        format!("{gbps:.1} GB/s"),
    )
}

/// Pure device time per pipeline stage (CUDA event timestamps, profiling
/// mode) — host enqueue overhead excluded. The kernel-vs-kernel numbers.
fn bench_stages(capacity: usize, dim: usize, degree: usize, ptx: Option<&str>) -> Vec<Row> {
    let mut backend = seeded_backend(capacity, dim, degree, 256, ptx);
    backend.enable_profiling();
    let query: Vec<f32> = (0..dim).map(|i| (i % 5) as f32 - 2.0).collect();

    let mut run_once = || {
        let fence = backend
            .topk(&query, 64, Some(256), None)
            .expect("topk+expand");
        while !fence.is_complete() {
            std::hint::spin_loop();
        }
        let fence = backend.materialize_topk().expect("materialize");
        while !fence.is_complete() {
            std::hint::spin_loop();
        }
        backend
            .last_selection_timings()
            .expect("timings probe")
            .expect("pipeline complete")
    };

    for _ in 0..WARMUP {
        run_once();
    }
    let mut sums = [0.0f64; 8];
    for _ in 0..STAGE_ITERS {
        let timings = run_once();
        for (sum, value) in sums.iter_mut().zip([
            Some(timings.selection_us),
            timings.materialize_us,
            timings.stage_query_us,
            timings.score_us,
            timings.block_topk_us,
            timings.merge_us,
            timings.seed_us,
            timings.expand_us,
        ]) {
            *sum += value.expect("profiling populates every stage");
        }
    }
    let labels = [
        "device:selection",
        "device:materialize",
        "stage:query+zero",
        "stage:score",
        "stage:block_topk",
        "stage:merge",
        "stage:seed",
        "stage:expand",
    ];
    let context = format!("cap={capacity} k=64 F={degree}");
    labels
        .iter()
        .zip(sums)
        .map(|(label, sum)| {
            Row::new(
                format!("{label} {context}"),
                sum / f64::from(STAGE_ITERS),
                "",
            )
        })
        .collect()
}

/// Ramps GPU boost clocks with sustained selection load before anything is
/// measured. Without this the first-run suite is systematically slower
/// (cold clocks), which `--compare` would misread as a kernel difference —
/// an identical module showed a spurious 5-12% "win" from ordering alone.
fn warm_device() {
    let mut backend = seeded_backend(64 << 10, 16, 32, 256, None);
    let query: Vec<f32> = (0..16).map(|i| (i % 5) as f32 - 2.0).collect();
    let start = Instant::now();
    while start.elapsed().as_secs_f64() < 3.0 {
        let fence = backend.topk(&query, 64, Some(256), None).expect("warm");
        while !fence.is_complete() {
            std::hint::spin_loop();
        }
    }
}

/// The selection-kernel-sensitive suite: everything a replacement module
/// changes. `--compare` runs exactly this, twice, on identically seeded
/// state.
fn selection_rows(ptx: Option<&str>) -> Vec<Row> {
    let mut rows = Vec::new();
    for capacity in [1usize << 10, 8 << 10, 64 << 10] {
        rows.extend(bench_selection(capacity, 16, 32, ptx));
    }
    rows.push(bench_materialize(64 << 10, ptx));
    rows.extend(bench_stages(64 << 10, 16, 32, ptx));
    rows
}

/// Tier machinery (promotion, append/flush): kernel-independent, reported
/// in single-module runs only.
fn machinery_rows() -> Vec<Row> {
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
    let mut rows = Vec::new();

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
    rows.push(Row::new(
        format!("promotion {promoted}x64KiB"),
        secs * 1e6,
        format!("{mbps:.1} MB/s"),
    ));

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
    rows.push(Row::new("append mean", total / APPENDS as f64, ""));
    rows.push(Row::new("append worst", worst, ""));
    rows.push(Row::new("append flush", flush_micros, ""));
    rows
}

fn print_rows(rows: &[Row]) {
    let width = rows.iter().map(|r| r.label.len()).max().unwrap_or(0);
    for row in rows {
        let note = if row.note.is_empty() {
            String::new()
        } else {
            format!("   {}", row.note)
        };
        println!(
            "{:<width$}  {:>10.1} us{note}",
            row.label,
            row.micros,
            width = width
        );
    }
}

fn json_escape_free(label: &str) -> &str {
    // Labels are generated above and contain no quotes or backslashes; keep
    // the emitter honest anyway.
    assert!(
        !label.contains('"') && !label.contains('\\'),
        "label needs JSON escaping: {label}"
    );
    label
}

fn print_json(module: &str, rows: &[Row]) {
    println!("{{");
    println!("  \"module\": \"{}\",", json_escape_free(module));
    println!("  \"results\": [");
    for (index, row) in rows.iter().enumerate() {
        let comma = if index + 1 == rows.len() { "" } else { "," };
        println!(
            "    {{\"label\": \"{}\", \"micros\": {:.3}}}{comma}",
            json_escape_free(&row.label),
            row.micros
        );
    }
    println!("  ]");
    println!("}}");
}

fn print_compare(baseline: &[Row], custom: &[Row], json: bool) {
    assert_eq!(baseline.len(), custom.len(), "suites diverged");
    if json {
        println!("[");
        for (index, (base, cust)) in baseline.iter().zip(custom).enumerate() {
            assert_eq!(base.label, cust.label, "suites diverged");
            let comma = if index + 1 == baseline.len() { "" } else { "," };
            println!(
                "  {{\"label\": \"{}\", \"baseline_us\": {:.3}, \"custom_us\": {:.3}}}{comma}",
                json_escape_free(&base.label),
                base.micros,
                cust.micros
            );
        }
        println!("]");
        return;
    }
    let width = baseline.iter().map(|r| r.label.len()).max().unwrap_or(0);
    println!(
        "{:<width$}  {:>12} {:>12} {:>8}",
        "op",
        "baseline",
        "custom",
        "delta",
        width = width
    );
    for (base, cust) in baseline.iter().zip(custom) {
        assert_eq!(base.label, cust.label, "suites diverged");
        let delta = (cust.micros - base.micros) / base.micros * 100.0;
        println!(
            "{:<width$}  {:>9.1} us {:>9.1} us {:>+7.1}%",
            base.label,
            base.micros,
            cust.micros,
            delta,
            width = width
        );
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: microbench [--ptx FILE] [--compare] [--json]\n\
         \n\
         --ptx FILE   register FILE as the selection module (in place of the baseline)\n\
         --compare    run baseline AND --ptx module on identically seeded state, print deltas\n\
         --json       machine-readable output\n\
         \n\
         Without --compare the run also includes the kernel-independent tier machinery\n\
         benches (promotion, append/flush)."
    );
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut ptx_path: Option<String> = None;
    let mut compare = false;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--ptx" => {
                index += 1;
                match args.get(index) {
                    Some(path) => ptx_path = Some(path.clone()),
                    None => usage(),
                }
            }
            "--compare" => compare = true,
            "--json" => json = true,
            _ => usage(),
        }
        index += 1;
    }
    if compare && ptx_path.is_none() {
        eprintln!("--compare needs --ptx FILE (something to compare against the baseline)");
        std::process::exit(2);
    }
    let ptx = ptx_path.as_ref().map(|path| {
        std::fs::read_to_string(path).unwrap_or_else(|error| {
            eprintln!("cannot read {path}: {error}");
            std::process::exit(2);
        })
    });

    if compare {
        let source = ptx.as_deref().expect("checked above");
        if !json {
            println!("== gpu-cache selection benches: baseline vs {} ==", {
                ptx_path.as_deref().unwrap_or("custom")
            });
        }
        warm_device();
        let baseline = selection_rows(None);
        let custom = selection_rows(Some(source));
        print_compare(&baseline, &custom, json);
        return;
    }

    let module = ptx_path.as_deref().unwrap_or("baseline");
    warm_device();
    let mut rows = selection_rows(ptx.as_deref());
    rows.extend(machinery_rows());
    if json {
        print_json(module, &rows);
    } else {
        println!(
            "== gpu-cache microbenches (module: {module}; budget: select+expand+gather <= 400 us) =="
        );
        print_rows(&rows);
    }
}
