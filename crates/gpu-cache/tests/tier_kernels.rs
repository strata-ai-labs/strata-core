//! GT3 exit gate: the CUDA selection kernels against the host-sim oracle.
//!
//! Both backends are loaded with byte-identical state (validity, tags,
//! f32 summaries, adjacency rows) and asked identical queries. Summaries
//! and queries are integer-valued floats, so dot products are exact in f32
//! regardless of accumulation hardware — selection results must match
//! *bitwise*, not approximately.
//!
//! ```bash
//! cargo test -p strata-gpu-cache --test tier_kernels -- --ignored
//! ```

use strata_gpu_cache::tier::backend::{
    scratch_bytes, DeviceBackend, Region, RegionBytes, TagFilter, TopkReadback,
};
use strata_gpu_cache::tier::host_sim::HostSimBackend;
use strata_gpu_cache::tier::CudaBackend;

const CAPACITY: usize = 128;
const DIM: usize = 16;
const DEGREE: usize = 4;

fn region_bytes() -> RegionBytes {
    RegionBytes {
        pages: (CAPACITY as u64) * 256,
        summaries: (CAPACITY * DIM * 4) as u64,
        adjacency: (CAPACITY * DEGREE * 4) as u64,
        validity: CAPACITY as u64,
        tags: (CAPACITY * 32) as u64,
        scratch: scratch_bytes(CAPACITY as u64, (DIM * 4) as u64),
        materialize: 64 * 256, // MAX_K pages of 256 bytes
    }
}

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

/// Writes identical randomized structures into both backends.
fn seed_state<B: DeviceBackend>(backend: &mut B, rng_seed: u64) {
    backend.reserve(region_bytes()).expect("reserve");
    let mut rng = Lcg(rng_seed);

    for slot in 0..CAPACITY {
        let valid = u8::from(rng.next() % 4 != 0); // ~75% valid
        backend
            .copy_in(Region::Validity, slot as u64, &[valid])
            .expect("validity");

        let mut tags = [0u8; 32];
        tags[0..8].copy_from_slice(&(rng.next() % 3).to_le_bytes());
        tags[8..16].copy_from_slice(&(slot as u64).to_le_bytes());
        backend
            .copy_in(Region::Tags, (slot * 32) as u64, &tags)
            .expect("tags");

        // Integer-valued f32 summaries in [-8, 8): exact dot products.
        let mut summary = Vec::with_capacity(DIM * 4);
        for _ in 0..DIM {
            #[allow(clippy::cast_precision_loss)]
            let value = (rng.next() % 16) as f32 - 8.0;
            summary.extend_from_slice(&value.to_le_bytes());
        }
        backend
            .copy_in(Region::Summaries, (slot * DIM * 4) as u64, &summary)
            .expect("summary");

        // Adjacency: each entry randomly links a slot or stays empty.
        let mut row = Vec::with_capacity(DEGREE * 4);
        for _ in 0..DEGREE {
            let entry = if rng.next() % 3 == 0 {
                u32::MAX
            } else {
                u32::try_from(rng.next() % CAPACITY as u64).unwrap()
            };
            row.extend_from_slice(&entry.to_le_bytes());
        }
        backend
            .copy_in(Region::Adjacency, (slot * DEGREE * 4) as u64, &row)
            .expect("adjacency");
    }
}

fn query_from(rng: &mut Lcg) -> Vec<f32> {
    (0..DIM)
        .map(|_| {
            #[allow(clippy::cast_precision_loss)]
            let value = (rng.next() % 16) as f32 - 8.0;
            value
        })
        .collect()
}

fn run_both(
    cuda: &mut CudaBackend,
    sim: &mut HostSimBackend,
    query: &[f32],
    k: u16,
    expand: Option<u16>,
    filter: Option<TagFilter>,
) -> (TopkReadback, TopkReadback) {
    cuda.topk(query, k, expand, filter).expect("cuda topk");
    sim.topk(query, k, expand, filter).expect("sim topk");
    (
        cuda.read_topk().expect("cuda read"),
        sim.read_topk().expect("sim read"),
    )
}

#[test]
#[ignore = "requires an NVIDIA GPU (Ampere or newer)"]
fn kernels_match_the_oracle_bitwise() {
    let mut cuda = CudaBackend::new(1 << 16).expect("device present");
    let mut sim = HostSimBackend::new();
    seed_state(&mut cuda, 0x00C0_FFEE);
    seed_state(&mut sim, 0x00C0_FFEE);

    let mut rng = Lcg(0xBEEF);
    for case in 0..24 {
        let query = query_from(&mut rng);
        let k = u16::try_from(1 + (case % 12)).unwrap();
        let filter = match case % 3 {
            0 => None,
            _ => Some(TagFilter {
                index: 0,
                value: rng.next() % 3,
            }),
        };
        let (gpu, oracle) = run_both(&mut cuda, &mut sim, &query, k, None, filter);
        assert_eq!(
            gpu.selected, oracle.selected,
            "case {case}: selection must match the oracle bitwise (k={k}, filter={filter:?})"
        );
    }
}

#[test]
#[ignore = "requires an NVIDIA GPU (Ampere or newer)"]
fn selection_pads_when_fewer_slots_qualify() {
    let mut cuda = CudaBackend::new(1 << 16).expect("device present");
    let mut sim = HostSimBackend::new();
    seed_state(&mut cuda, 7);
    seed_state(&mut sim, 7);

    // A filter value that matches roughly a third of slots, with k larger
    // than the match count: both sides must return exactly the qualifiers.
    let query = vec![1.0f32; DIM];
    let filter = Some(TagFilter { index: 0, value: 2 });
    let (gpu, oracle) = run_both(&mut cuda, &mut sim, &query, 64, None, filter);
    assert_eq!(gpu.selected, oracle.selected);
    assert!(gpu.selected.len() < 64, "padding path exercised");
}

#[test]
#[ignore = "requires an NVIDIA GPU (Ampere or newer)"]
fn expansion_matches_the_oracle_as_a_set() {
    let mut cuda = CudaBackend::new(1 << 16).expect("device present");
    let mut sim = HostSimBackend::new();
    seed_state(&mut cuda, 0xDA7A);
    seed_state(&mut sim, 0xDA7A);

    let mut rng = Lcg(0x51DE);
    for case in 0..12 {
        let query = query_from(&mut rng);
        // A generous budget never truncates: the expanded *set* is
        // deterministic even though kernel output order is not.
        let (gpu, oracle) = run_both(&mut cuda, &mut sim, &query, 8, Some(256), None);
        assert_eq!(gpu.selected, oracle.selected, "case {case}: selection");
        let mut gpu_set = gpu.expanded.clone();
        let mut oracle_set = oracle.expanded.clone();
        gpu_set.sort_unstable();
        oracle_set.sort_unstable();
        assert_eq!(gpu_set, oracle_set, "case {case}: expansion set");
    }
}

#[test]
#[ignore = "requires an NVIDIA GPU (Ampere or newer)"]
fn truncated_expansion_stays_within_the_oracle_superset() {
    let mut cuda = CudaBackend::new(1 << 16).expect("device present");
    let mut sim = HostSimBackend::new();
    seed_state(&mut cuda, 0xACE);
    seed_state(&mut sim, 0xACE);

    let query = vec![2.0f32; DIM];
    // Untruncated oracle superset first.
    sim.topk(&query, 8, Some(256), None).expect("sim");
    let superset: std::collections::HashSet<u32> = sim
        .read_topk()
        .expect("read")
        .expanded
        .into_iter()
        .collect();

    // A tight budget on the GPU: any subset of the superset, exactly budget
    // in size (when the superset is larger).
    cuda.topk(&query, 8, Some(2), None).expect("cuda");
    let truncated = cuda.read_topk().expect("read").expanded;
    if superset.len() >= 2 {
        assert_eq!(truncated.len(), 2, "budget bound respected");
    }
    for slot in truncated {
        assert!(
            superset.contains(&slot),
            "budgeted entries come from the true expansion"
        );
    }
}

#[test]
#[ignore = "requires an NVIDIA GPU (Ampere or newer)"]
fn materialized_pages_match_the_oracle() {
    let mut cuda = CudaBackend::new(1 << 16).expect("device present");
    let mut sim = HostSimBackend::new();
    seed_state(&mut cuda, 0x9A6E5);
    seed_state(&mut sim, 0x9A6E5);

    // Give each slot's page distinct content so gather order is provable.
    for slot in 0..CAPACITY {
        let fill = u8::try_from(slot % 251).unwrap();
        let bytes = vec![fill; 256];
        cuda.copy_in(Region::Pages, (slot * 256) as u64, &bytes)
            .expect("cuda page");
        sim.copy_in(Region::Pages, (slot * 256) as u64, &bytes)
            .expect("sim page");
    }

    let mut rng = Lcg(0x60D);
    for case in 0..6 {
        let query = query_from(&mut rng);
        let k = 8u16;
        let (gpu, oracle) = run_both(&mut cuda, &mut sim, &query, k, None, None);
        assert_eq!(gpu.selected, oracle.selected, "case {case}: selection");

        cuda.materialize_topk().expect("cuda materialize");
        sim.materialize_topk().expect("sim materialize");
        let len = usize::from(k) * 256;
        let gpu_bytes = cuda
            .read_back(Region::Materialize, 0, len)
            .expect("gpu read");
        let sim_bytes = sim
            .read_back(Region::Materialize, 0, len)
            .expect("sim read");
        assert_eq!(gpu_bytes, sim_bytes, "case {case}: materialized bytes");
    }
}
