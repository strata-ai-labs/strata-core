//! The kernel registration seam (design D3): a replacement selection
//! module drives real selections, and contract violations fail at open,
//! never at the first decode step.
//!
//! The "custom" module here is the baseline source registered through the
//! override path — the point under test is the seam (registered source is
//! the one loaded, eager entry resolution enforces the contract), not new
//! kernel math; the oracle-equivalence suite (`tier_kernels`) covers
//! semantics for whatever module is installed.
//!
//! ```bash
//! cargo test -p strata-gpu-cache --test tier_custom_kernels -- --ignored
//! ```

use strata_gpu_cache::tier::backend::{
    scratch_bytes, CopyFence, DeviceBackend, Region, RegionBytes,
};
use strata_gpu_cache::tier::CudaBackend;
use strata_gpu_cache::BASELINE_SELECTION_PTX;

const CAPACITY: usize = 64;
const DIM: usize = 4;

fn region_bytes() -> RegionBytes {
    RegionBytes {
        pages: (CAPACITY as u64) * 256,
        summaries: (CAPACITY * DIM * 4) as u64,
        adjacency: (CAPACITY * 4 * 4) as u64,
        validity: CAPACITY as u64,
        tags: (CAPACITY * 32) as u64,
        scratch: scratch_bytes(CAPACITY as u64, (DIM * 4) as u64),
        materialize: 64 * 256,
    }
}

#[test]
#[ignore = "requires an NVIDIA GPU"]
fn registered_module_drives_selection() {
    let mut backend = CudaBackend::new(1 << 16).expect("device present");
    backend
        .register_selection_ptx(BASELINE_SELECTION_PTX)
        .expect("register before reserve");
    backend.reserve(region_bytes()).expect("reserve");

    // One valid slot with a distinctive summary; selection must find it.
    backend
        .copy_in(Region::Validity, 7, &[1])
        .expect("validity");
    let summary: Vec<u8> = [3.0f32, 0.0, 0.0, 0.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    backend
        .copy_in(Region::Summaries, (7 * DIM * 4) as u64, &summary)
        .expect("summary");

    let fence = backend
        .topk(&[2.0, 0.0, 0.0, 0.0], 4, None, None)
        .expect("topk");
    while !fence.is_complete() {
        std::hint::spin_loop();
    }
    let readback = backend.read_topk().expect("read_topk");
    assert_eq!(readback.selected, vec![(7, 6.0)]);
}

#[test]
#[ignore = "requires an NVIDIA GPU"]
fn missing_entry_fails_at_reserve() {
    // Rename one required entry point: eager resolution must reject the
    // module when the tier opens, proving the registered source (not the
    // baseline) is what gets loaded.
    let broken = BASELINE_SELECTION_PTX.replace(
        ".visible .entry gather_pages(",
        ".visible .entry gather_pagez(",
    );
    let mut backend = CudaBackend::new(1 << 16).expect("device present");
    backend
        .register_selection_ptx(broken)
        .expect("register before reserve");
    let error = backend.reserve(region_bytes()).expect_err("missing entry");
    assert!(
        error.to_string().contains("cuModuleGetFunction"),
        "unexpected error: {error}"
    );
}

#[test]
#[ignore = "requires an NVIDIA GPU"]
fn registration_after_reserve_is_refused() {
    let mut backend = CudaBackend::new(1 << 16).expect("device present");
    backend.reserve(region_bytes()).expect("reserve");
    let error = backend
        .register_selection_ptx(BASELINE_SELECTION_PTX)
        .expect_err("too late");
    assert!(
        error.to_string().contains("before reserve"),
        "unexpected error: {error}"
    );
}

#[test]
#[ignore = "requires an NVIDIA GPU"]
fn non_ascii_ptx_is_refused() {
    let mut backend = CudaBackend::new(1 << 16).expect("device present");
    backend
        .register_selection_ptx("// né pas du PTX")
        .expect("registration stores the source");
    let error = backend.reserve(region_bytes()).expect_err("non-ascii");
    assert!(
        error.to_string().contains("ASCII"),
        "unexpected error: {error}"
    );
}
