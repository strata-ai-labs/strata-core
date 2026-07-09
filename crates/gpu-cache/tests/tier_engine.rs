//! GT2 exit gate: the tier over the real store of record.
//!
//! Runs in CI (no GPU — the host-sim backend carries the device side; the
//! engine database on a temp dir carries T2). Proves the durable contract:
//! flush is the durability point, reopen validates geometry and continues
//! the id watermark, cold pages promote back out of the durable store, and
//! unflushed appends are lost on reopen (bounded, observable loss — the
//! documented crash semantics).

use strata_gpu_cache::tier::backend::{DeviceBackend, Region};
use strata_gpu_cache::tier::engine_store::EnginePageStore;
use strata_gpu_cache::tier::host_sim::HostSimBackend;
use strata_gpu_cache::tier::page_table::PageId;
use strata_gpu_cache::tier::store::PageBlob;
use strata_gpu_cache::tier::tier::{Tier, TierConfig};

const PAGE_BYTES: u64 = 256;
const SUMMARY_BYTES: u64 = 64;

fn config() -> TierConfig {
    TierConfig {
        page_bytes: PAGE_BYTES,
        summary_bytes: SUMMARY_BYTES,
        page_slots: 4,
        promotion_batch: 4,
        adjacency_degree: 8,
        write_behind_batch: 2,
        write_backlog_cap: 8,
    }
}

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
fn flush_reopen_promote_round_trip() {
    let dir = tempfile::tempdir().expect("temp dir");

    // Session 1: append four pages, flush, close.
    let (first_ids, receipt) = {
        let store = EnginePageStore::open(dir.path(), "tier").expect("store opens");
        let mut tier = Tier::open(HostSimBackend::new(), store, config()).expect("tier opens");
        let ids: Vec<PageId> = (0..4)
            .map(|i| tier.append(&blob_for(i)).expect("append"))
            .collect();
        tier.maintain();
        let receipt = tier.flush().expect("flush").expect("durability point");
        tier.store_mut().close().expect("close");
        (ids, receipt)
    };
    assert!(receipt.version > 0);
    assert_eq!(first_ids, vec![PageId(0), PageId(1), PageId(2), PageId(3)]);

    // Session 2: geometry validates, watermark continues, pages promote
    // back out of the durable store with their exact bytes.
    let store = EnginePageStore::open(dir.path(), "tier").expect("store reopens");
    let mut tier = Tier::open(
        HostSimBackend::new(),
        store,
        TierConfig {
            page_slots: 8, // room for the four promotions plus the new append
            ..config()
        },
    )
    .expect("tier reopens");

    let next = tier.append(&blob_for(99)).expect("append");
    assert_eq!(next, PageId(4), "ids continue past the durable watermark");

    for id in &first_ids {
        tier.request(*id, 1);
    }
    for _ in 0..4 {
        tier.maintain();
    }
    for (index, id) in first_ids.iter().enumerate() {
        assert!(tier.is_selectable(*id), "page {index} promoted from T2");
    }
    // Byte identity through the full loop: durable rows -> promotion ->
    // device region. The append took slot 0; equal-priority promotions are
    // FIFO, so ids 0..3 land in slots 1..4.
    for (index, id) in (0u64..4).enumerate() {
        let slot = index as u64 + 1;
        let bytes = tier
            .backend_mut()
            .read_back(
                Region::Pages,
                slot * PAGE_BYTES,
                usize::try_from(PAGE_BYTES).unwrap(),
            )
            .expect("read back");
        assert_eq!(bytes, blob_for(id).bytes, "page {id} bytes round-tripped");
    }
    tier.store_mut().close().expect("close");
}

#[test]
fn geometry_change_is_refused_on_reopen() {
    let dir = tempfile::tempdir().expect("temp dir");
    {
        let store = EnginePageStore::open(dir.path(), "tier").expect("store opens");
        let mut tier = Tier::open(HostSimBackend::new(), store, config()).expect("tier opens");
        tier.flush().expect("flush");
        tier.store_mut().close().expect("close");
    }
    let store = EnginePageStore::open(dir.path(), "tier").expect("store reopens");
    let result = Tier::open(
        HostSimBackend::new(),
        store,
        TierConfig {
            page_bytes: PAGE_BYTES * 2, // different geometry
            ..config()
        },
    );
    let Err(error) = result else {
        panic!("mismatched geometry must refuse");
    };
    assert_eq!(error.code(), "failed_precondition.tier.geometry_mismatch");
}

#[test]
fn unflushed_appends_are_lost_on_reopen_bounded_and_observable() {
    let dir = tempfile::tempdir().expect("temp dir");
    let orphan = {
        let store = EnginePageStore::open(dir.path(), "tier").expect("store opens");
        let mut tier = Tier::open(HostSimBackend::new(), store, config()).expect("tier opens");
        // Two flushed pages, then one append below the batch size, no flush.
        tier.append(&blob_for(0)).expect("append");
        tier.append(&blob_for(1)).expect("append");
        tier.maintain(); // batch of 2 commits
        let orphan = tier.append(&blob_for(2)).expect("append");
        assert_eq!(tier.write_backlog(), 1, "one entry never committed");
        tier.store_mut().close().expect("close");
        orphan
    };

    let store = EnginePageStore::open(dir.path(), "tier").expect("store reopens");
    let mut tier = Tier::open(HostSimBackend::new(), store, config()).expect("tier reopens");
    // The flushed pages exist; the orphan does not (a store miss, counted).
    tier.request(PageId(0), 1);
    tier.request(orphan, 1);
    for _ in 0..4 {
        tier.maintain();
    }
    assert!(tier.is_selectable(PageId(0)), "flushed page survives");
    assert!(!tier.is_selectable(orphan), "unflushed page is gone");
    assert_eq!(tier.stats().store_misses, 1, "the loss is observable");
    // And its id is never reissued: the watermark only covers committed ids,
    // so the next append reuses the orphan's id — documented single-writer
    // semantics (the tier is the only writer of its space).
    let reissued = tier.append(&blob_for(9)).expect("append");
    assert_eq!(reissued, orphan, "orphaned id is reissued after the crash");
    tier.store_mut().close().expect("close");
}
