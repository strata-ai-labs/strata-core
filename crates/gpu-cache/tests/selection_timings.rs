//! The selection-timing endpoint: device-time measurements for kernel
//! benchmarking (the Moho A/B surface), probed without host syncs.
//!
//! ```bash
//! cargo test -p strata-gpu-cache --test selection_timings -- --ignored
//! ```

use strata_gpu_cache::tier::backend::{
    scratch_bytes, CopyFence, DeviceBackend, Region, RegionBytes,
};
use strata_gpu_cache::tier::CudaBackend;

const CAPACITY: usize = 512;
const DIM: usize = 4;
const DEGREE: usize = 4;
const SEEDED: usize = 16;

fn region_bytes() -> RegionBytes {
    RegionBytes {
        pages: (CAPACITY as u64) * 256,
        summaries: (CAPACITY * DIM * 4) as u64,
        adjacency: (CAPACITY * DEGREE * 4) as u64,
        validity: CAPACITY as u64,
        tags: (CAPACITY * 32) as u64,
        scratch: scratch_bytes(CAPACITY as u64, (DIM * 4) as u64),
        materialize: 64 * 256,
    }
}

/// A backend with a handful of selectable slots whose adjacency points
/// inside the seeded set (expansion never dereferences garbage rows: it
/// only walks rows of selected — hence seeded — slots).
fn seeded_backend(profile: bool) -> CudaBackend {
    let mut backend = CudaBackend::new(1 << 16).expect("device present");
    if profile {
        backend.enable_profiling();
    }
    backend.reserve(region_bytes()).expect("reserve");
    backend
        .copy_in(Region::Validity, 0, &[1u8; SEEDED])
        .expect("validity");
    let mut summaries = Vec::new();
    for slot in 0..SEEDED {
        for lane in 0..DIM {
            let value = f32::from(u8::try_from((slot + lane) % 5).expect("< 5"));
            summaries.extend_from_slice(&value.to_le_bytes());
        }
    }
    backend
        .copy_in(Region::Summaries, 0, &summaries)
        .expect("summaries");
    let mut adjacency = Vec::new();
    for slot in 0..SEEDED {
        for edge in 0..DEGREE {
            let entry = if edge == 0 {
                u32::try_from((slot + 1) % SEEDED).expect("fits")
            } else {
                u32::MAX
            };
            adjacency.extend_from_slice(&entry.to_le_bytes());
        }
    }
    backend
        .copy_in(Region::Adjacency, 0, &adjacency)
        .expect("adjacency");
    backend
}

fn spin<F: CopyFence>(fence: &F) {
    while !fence.is_complete() {
        std::hint::spin_loop();
    }
}

fn query() -> Vec<f32> {
    (0..DIM)
        .map(|lane| f32::from(u8::try_from(lane % 3).expect("< 3")))
        .collect()
}

#[test]
#[ignore = "requires an NVIDIA GPU"]
fn totals_available_after_completion_without_syncs() {
    let mut backend = seeded_backend(false);
    assert!(
        backend.last_selection_timings().expect("probe").is_none(),
        "no timings before any topk"
    );

    let syncs_before = backend.context().sync_call_count();
    let fence = backend.topk(&query(), 4, None, None).expect("topk");
    spin(&fence);
    let timings = backend
        .last_selection_timings()
        .expect("probe")
        .expect("selection complete");
    assert!(timings.selection_us > 0.0, "device time measured");
    assert_eq!(timings.materialize_us, None, "no materialize enqueued yet");
    assert_eq!(
        timings.score_us, None,
        "stage breakdown requires profiling mode"
    );

    let fence = backend.materialize_topk().expect("materialize");
    spin(&fence);
    let timings = backend
        .last_selection_timings()
        .expect("probe")
        .expect("still complete");
    assert!(
        timings.materialize_us.expect("gather measured") > 0.0,
        "materialize device time measured"
    );
    assert_eq!(
        backend.context().sync_call_count(),
        syncs_before,
        "timing probes must never issue a host sync"
    );
}

#[test]
#[ignore = "requires an NVIDIA GPU"]
fn profiling_reports_stage_breakdown() {
    let mut backend = seeded_backend(true);
    let fence = backend.topk(&query(), 4, Some(8), None).expect("topk");
    spin(&fence);
    let timings = backend
        .last_selection_timings()
        .expect("probe")
        .expect("selection complete");
    let stages = [
        ("stage_query", timings.stage_query_us),
        ("score", timings.score_us),
        ("block_topk", timings.block_topk_us),
        ("merge", timings.merge_us),
        ("seed", timings.seed_us),
        ("expand", timings.expand_us),
    ];
    for (name, value) in stages {
        assert!(
            value.is_some_and(|micros| micros >= 0.0),
            "{name} missing from the profile breakdown: {value:?}"
        );
    }
    let sum: f64 = stages.iter().filter_map(|(_, value)| *value).sum();
    assert!(
        sum <= timings.selection_us + 10.0,
        "stage sum {sum:.1} us exceeds pipeline total {:.1} us",
        timings.selection_us
    );

    // Without an expansion budget the expansion stages never run — their
    // fields must vanish, and the previous selection's values must not
    // leak through.
    let fence = backend.topk(&query(), 4, None, None).expect("topk");
    spin(&fence);
    let timings = backend
        .last_selection_timings()
        .expect("probe")
        .expect("selection complete");
    assert!(timings.score_us.is_some(), "profiling stays on");
    assert_eq!(timings.seed_us, None, "no expansion requested");
    assert_eq!(timings.expand_us, None, "no expansion requested");
    assert_eq!(timings.materialize_us, None, "reset at the next topk");
}
