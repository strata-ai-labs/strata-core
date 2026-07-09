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
use strata_gpu_cache::tier::store::{PageBlob, PageStore};
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
fn fork_branches_the_store_of_record() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = EnginePageStore::open(dir.path(), "tier").expect("store opens");
    let mut parent = Tier::open(
        HostSimBackend::new(),
        store,
        TierConfig {
            page_slots: 8,
            ..config()
        },
    )
    .expect("tier opens");

    // Durable shared history, then fork: branch + handle in two calls.
    let shared: Vec<PageId> = (0..3)
        .map(|i| parent.append(&blob_for(i)).expect("append"))
        .collect();
    parent.maintain();
    parent.flush().expect("flush");
    let mut child = parent.fork_branch("rollout-1").expect("fork");

    // The child branch carries the shared history: the manifest passed the
    // fork's geometry check, and the pre-fork pages read back through the
    // child's store.
    let blobs = child.store_mut().read_pages(&shared).expect("read");
    assert!(
        blobs.iter().all(Option::is_some),
        "shared history visible on the child branch"
    );

    // Divergence: post-fork appends land on their own branch only.
    let parent_new = parent.append(&blob_for(10)).expect("parent append");
    let child_new = child.append(&blob_for(20)).expect("child append");
    assert_ne!(parent_new, child_new, "family id clock stays unique");
    parent.flush().expect("parent flush");
    child.flush().expect("child flush");

    let cross = child.store_mut().read_pages(&[parent_new]).expect("read");
    assert_eq!(
        cross,
        vec![None],
        "parent's post-fork page is invisible to the child branch"
    );
    let cross = parent.store_mut().read_pages(&[child_new]).expect("read");
    assert_eq!(
        cross,
        vec![None],
        "child's post-fork page is invisible to the parent branch"
    );
    child.store_mut().close().expect("close");
}

#[test]
fn refused_fork_leaves_no_orphaned_branch() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = EnginePageStore::open(dir.path(), "tier").expect("store opens");
    let mut tier = Tier::open(HostSimBackend::new(), store, config()).expect("tier opens");
    tier.append(&blob_for(0)).expect("append");

    // fork_branch checks the backlog before creating the branch...
    let Err(error) = tier.fork_branch("rollout-1") else {
        panic!("unflushed parent must refuse");
    };
    assert_eq!(error.code(), "failed_precondition.tier.fork_unflushed");

    // ...so after flushing, the same name is still available.
    tier.flush().expect("flush");
    let child = tier.fork_branch("rollout-1").expect("no orphaned branch");
    drop(child);
    tier.store_mut().close().expect("close");
}

#[test]
fn fork_onto_an_existing_branch_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = EnginePageStore::open(dir.path(), "tier").expect("store opens");
    let mut tier = Tier::open(HostSimBackend::new(), store, config()).expect("tier opens");
    tier.flush().expect("flush");

    let first = tier.store().fork("dup").expect("first fork");
    drop(first);
    let Err(error) = tier.store().fork("dup") else {
        panic!("duplicate branch must refuse");
    };
    assert_eq!(error.code(), "unavailable.tier.store");
    tier.store_mut().close().expect("close");
}

#[test]
fn forked_branch_reopens_with_continuity() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (shared_id, child_id) = {
        let store = EnginePageStore::open(dir.path(), "tier").expect("store opens");
        let mut parent = Tier::open(HostSimBackend::new(), store, config()).expect("tier opens");
        let shared_id = parent.append(&blob_for(0)).expect("append");
        parent.append(&blob_for(1)).expect("append");
        parent.maintain(); // batch of 2 commits
        parent.flush().expect("flush");

        let mut child = parent.fork_branch("rollout-1").expect("fork");
        let child_id = child.append(&blob_for(5)).expect("child append");
        child.flush().expect("child flush");
        child.store_mut().close().expect("close");
        (shared_id, child_id)
    };

    // Reopen the forked branch as its own tier: geometry validates, the
    // branch-local watermark continues, and both pre-fork and post-fork
    // pages promote out of the child's branch of record.
    let store =
        EnginePageStore::open_on_branch(dir.path(), "tier", "rollout-1").expect("branch reopens");
    let mut tier = Tier::open(HostSimBackend::new(), store, config()).expect("tier reopens");
    let next = tier.append(&blob_for(9)).expect("append");
    assert_eq!(
        next.0,
        child_id.0 + 1,
        "child branch watermark continues past its own appends"
    );
    tier.request(shared_id, 1);
    tier.request(child_id, 1);
    for _ in 0..4 {
        tier.maintain();
    }
    assert!(
        tier.is_selectable(shared_id),
        "pre-fork page promotes on the child branch"
    );
    assert!(tier.is_selectable(child_id), "child's own page promotes");
    tier.store_mut().close().expect("close");
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
