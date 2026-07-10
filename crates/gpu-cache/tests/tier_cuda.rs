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
        tags: [id, id * 2, 0, 0],
        edges: Vec::new(),
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
            adjacency_degree: 8,
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

/// HT-11 hardware mirror: fork a GPU tier over durable Strata, prove the
/// warm set is shared without device copies, then drive a family-wide
/// eviction so slot reuse crosses handles under real CUDA event fencing.
#[test]
#[ignore = "requires an NVIDIA GPU (Ampere or newer)"]
fn fork_family_on_real_hardware() {
    use strata_gpu_cache::tier::engine_store::EnginePageStore;

    let dir = tempfile::tempdir().expect("temp dir");
    let backend = CudaBackend::new(usize::try_from(PAGE_BYTES).unwrap()).expect("device");
    let store = EnginePageStore::open(dir.path(), "tier").expect("store opens");
    let mut parent = Tier::open(
        backend,
        store,
        TierConfig {
            page_bytes: PAGE_BYTES,
            summary_bytes: SUMMARY_BYTES,
            page_slots: 3,
            promotion_batch: 4,
            adjacency_degree: 8,
            write_behind_batch: 2,
            write_backlog_cap: 8,
        },
    )
    .expect("tier opens");

    // Warm set: three appended pages fill the pool; durable at flush.
    for id in 0..3 {
        parent.append(&blob_for(id)).expect("append");
    }
    for _ in 0..10_000 {
        parent.maintain();
        if (0..3).all(|id| parent.is_selectable(PageId(id))) {
            break;
        }
        std::thread::yield_now();
    }
    parent.flush().expect("flush");

    // Fork: branch + handle. Metadata only — the child starts warm with
    // zero promotions and the sync counter untouched.
    let syncs_before = parent.backend().context().sync_call_count();
    let mut child = parent.fork_branch("rollout-1").expect("fork");
    for id in 0..3 {
        assert!(child.is_selectable(PageId(id)), "page {id} warm in child");
    }
    assert_eq!(child.stats().promotions_started, 0, "no copies at fork");
    assert_eq!(
        parent.backend().context().sync_call_count(),
        syncs_before,
        "fork issues no device waits"
    );

    // Keep pages 1 and 2 hot in the child; family-wide eviction of page 0:
    // the parent's releases are shared (no device writes), the child's is
    // the last reference — its reuse gate rides a real CUDA event fence.
    child.touch(PageId(1), 1.0);
    child.touch(PageId(2), 1.0);
    parent.append(&blob_for(3)).expect("parent append"); // strips parent's shared view
    parent.flush().expect("parent flush");
    let child_page = child.append(&blob_for(4)).expect("child append");
    child.flush().expect("child flush");
    for _ in 0..10_000 {
        child.step_begin().expect("step");
        child.request(child_page, 1);
        child.maintain();
        if child.is_selectable(child_page) {
            break;
        }
        std::thread::yield_now();
    }
    assert!(
        child.is_selectable(child_page),
        "child's page landed after cross-handle fenced reuse"
    );
    assert!(child.is_selectable(PageId(1)), "hot shared page survived");
    assert!(!child.is_selectable(PageId(0)), "cold page left the union");

    // Byte identity through the fork: the child's new page and a shared
    // page both read back exactly from VRAM (two deliberate syncs).
    let slot_of = |tier: &Tier<CudaBackend, EnginePageStore>, id: u64| {
        (0..tier.capacity())
            .find(|slot| tier.page_of_slot(*slot) == Some(PageId(id)))
            .expect("page resident")
    };
    let new_slot = slot_of(&child, child_page.0);
    let shared_slot = slot_of(&child, 1);
    assert_eq!(
        parent.backend().context().sync_call_count(),
        syncs_before,
        "the whole fork/evict/reuse flow issued zero device waits"
    );
    for (slot, id) in [(new_slot, child_page.0), (shared_slot, 1)] {
        let bytes = child
            .backend_mut()
            .read_back(
                Region::Pages,
                u64::from(slot) * PAGE_BYTES,
                usize::try_from(PAGE_BYTES).unwrap(),
            )
            .expect("read back from VRAM");
        assert_eq!(bytes, blob_for(id).bytes, "page {id} bytes in VRAM");
    }

    child.store_mut().close().expect("close");
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
        adjacency_degree: 8,
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
