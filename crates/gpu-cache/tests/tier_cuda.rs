//! GT1 hardware mirror: the same tier flow the host-sim suite proves, run
//! against the real CUDA backend on the dev GPU.
//!
//! ```bash
//! cargo test -p strata-gpu-cache --test tier_cuda -- --ignored
//! ```

use strata_gpu_cache::tier::backend::{DeviceBackend, Region};
use strata_gpu_cache::tier::page_table::PageId;
use strata_gpu_cache::tier::store::{InMemoryStore, PageBlob};
use strata_gpu_cache::tier::tier::{RequestOutcome, Tier, TierConfig};
use strata_gpu_cache::tier::CudaBackend;

const PAGE_BYTES: u64 = 64 << 10;
const SUMMARY_BYTES: u64 = 256;

fn blob_for(id: u64) -> PageBlob {
    PageBlob {
        bytes: vec![u8::try_from(id % 251).unwrap(); usize::try_from(PAGE_BYTES).unwrap()],
        summary: vec![
            u8::try_from((id * 7) % 251).unwrap();
            usize::try_from(SUMMARY_BYTES).unwrap()
        ],
    }
}

#[test]
#[ignore = "requires an NVIDIA GPU (Ampere or newer)"]
fn tier_flow_on_real_hardware() {
    let mut store = InMemoryStore::new();
    for id in 0..8 {
        store.seed(PageId(id), blob_for(id));
    }
    let backend = CudaBackend::new(usize::try_from(PAGE_BYTES).unwrap()).expect("device present");
    let mut tier = Tier::open(
        backend,
        store,
        TierConfig {
            page_bytes: PAGE_BYTES,
            summary_bytes: SUMMARY_BYTES,
            page_slots: 4,
            promotion_batch: 4,
            write_behind_batch: 4,
            write_backlog_cap: 8,
        },
    )
    .expect("tier opens on hardware");

    // Promote four pages; poll to selectability (real async completion).
    for id in 0..4 {
        assert_eq!(tier.request(PageId(id), 1), RequestOutcome::Queued);
    }
    for _ in 0..10_000 {
        tier.maintain();
        if (0..4).all(|id| tier.is_selectable(PageId(id))) {
            break;
        }
        std::thread::yield_now();
    }
    for id in 0..4 {
        assert!(tier.is_selectable(PageId(id)), "page {id} resident on GPU");
    }

    // Bytes actually live in VRAM at the expected slots.
    for id in 0..4u64 {
        let bytes = tier
            .backend_mut()
            .read_back(
                Region::Pages,
                id * PAGE_BYTES,
                usize::try_from(PAGE_BYTES).unwrap(),
            )
            .expect("read back from VRAM");
        assert_eq!(bytes, blob_for(id).bytes, "page {id} bytes in VRAM");
    }

    // Evict under real event fencing: warm three pages, request a fourth.
    tier.step_begin().expect("step");
    for id in [0u64, 2, 3] {
        tier.touch(PageId(id), 1.0);
    }
    tier.request(PageId(4), 1);
    tier.maintain(); // stages the eviction of the cold page
    tier.step_begin().expect("step");
    for _ in 0..10_000 {
        tier.maintain();
        tier.request(PageId(4), 1);
        if tier.is_selectable(PageId(4)) {
            break;
        }
        std::thread::yield_now();
    }
    assert!(
        tier.is_selectable(PageId(4)),
        "new page landed after fenced reuse"
    );
    assert!(!tier.is_selectable(PageId(1)), "cold page evicted");
    assert_eq!(
        tier.capacity(),
        tier.free_now() + u32::try_from(tier.gated()).unwrap() + tier.resident(),
        "conservation holds on hardware"
    );

    // The decode-loop paths issued zero blocking waits; only read_back's
    // deliberate synchronizes are counted.
    let syncs = tier.backend().context().sync_call_count();
    assert_eq!(syncs, 4, "exactly the four deliberate read_back waits");
}

/// The full stack in one test: pages appended through the tier land in a
/// real durable Strata database (T2) and in real VRAM (T0); after a flush
/// and reopen they promote back onto the GPU with byte identity.
#[test]
#[ignore = "requires an NVIDIA GPU (Ampere or newer)"]
fn full_stack_gpu_over_durable_strata() {
    use strata_gpu_cache::tier::engine_store::EnginePageStore;

    let dir = tempfile::tempdir().expect("temp dir");
    let config = TierConfig {
        page_bytes: PAGE_BYTES,
        summary_bytes: SUMMARY_BYTES,
        page_slots: 4,
        promotion_batch: 4,
        write_behind_batch: 2,
        write_backlog_cap: 8,
    };

    // Session 1: append through the GPU tier, flush, close.
    {
        let backend = CudaBackend::new(usize::try_from(PAGE_BYTES).unwrap()).expect("device");
        let store = EnginePageStore::open(dir.path(), "tier").expect("store opens");
        let mut tier = Tier::open(backend, store, config).expect("tier opens");
        for id in 0..4 {
            tier.append(&blob_for(id)).expect("append");
        }
        tier.maintain();
        let receipt = tier.flush().expect("flush").expect("durability point");
        assert!(receipt.version > 0, "durable commit receipt");
        tier.store_mut().close().expect("close");
    }

    // Session 2: cold start; promote from the durable store into VRAM.
    let backend = CudaBackend::new(usize::try_from(PAGE_BYTES).unwrap()).expect("device");
    let store = EnginePageStore::open(dir.path(), "tier").expect("store reopens");
    let mut tier = Tier::open(backend, store, config).expect("tier reopens");
    for id in 0..4 {
        tier.request(PageId(id), 1);
    }
    for _ in 0..10_000 {
        tier.maintain();
        if (0..4).all(|id| tier.is_selectable(PageId(id))) {
            break;
        }
        std::thread::yield_now();
    }
    for id in 0..4u64 {
        assert!(tier.is_selectable(PageId(id)), "page {id} back in VRAM");
        let bytes = tier
            .backend_mut()
            .read_back(
                Region::Pages,
                id * PAGE_BYTES,
                usize::try_from(PAGE_BYTES).unwrap(),
            )
            .expect("read back from VRAM");
        assert_eq!(bytes, blob_for(id).bytes, "durable -> VRAM byte identity");
    }
    tier.store_mut().close().expect("close");
}
