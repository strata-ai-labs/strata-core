//! HT-11b exit gate: COW page-table fork on the host-sim backend.
//!
//! Runs in ordinary CI with no GPU. Covers the fork contracts (flushed
//! parent, forked store), metadata-only warm-set sharing, reference-counted
//! eviction across handles (shared release vs last-gated), fence-gated
//! reuse for the whole family, unique page ids after divergence, union
//! adjacency unlink soundness, and drop-time shared-reference release.

use strata_gpu_cache::tier::backend::{DeviceBackend, Region};
use strata_gpu_cache::tier::host_sim::HostSimBackend;
use strata_gpu_cache::tier::page_table::PageId;
use strata_gpu_cache::tier::store::{InMemoryStore, PageBlob, PageStore, TierManifest};
use strata_gpu_cache::tier::tier::{RequestOutcome, Tier, TierConfig};

const PAGE_BYTES: u64 = 256;
const SUMMARY_BYTES: u64 = 64;
const DEGREE: u16 = 8;

type SimTier = Tier<HostSimBackend, InMemoryStore>;

fn blob_with_edges(id: u64, edges: Vec<PageId>) -> PageBlob {
    PageBlob {
        bytes: vec![u8::try_from(id % 251).unwrap(); usize::try_from(PAGE_BYTES).unwrap()],
        summary: vec![
            u8::try_from((id * 7) % 251).unwrap();
            usize::try_from(SUMMARY_BYTES).unwrap()
        ],
        tags: [id, id * 2, 0, 0],
        edges,
    }
}

fn blob_for(id: u64) -> PageBlob {
    blob_with_edges(id, Vec::new())
}

fn tier_with(slots: u32, seeded: u64) -> SimTier {
    let mut store = InMemoryStore::new();
    for id in 0..seeded {
        store.seed(PageId(id), blob_for(id));
    }
    Tier::open(
        HostSimBackend::new(),
        store,
        TierConfig {
            page_bytes: PAGE_BYTES,
            summary_bytes: SUMMARY_BYTES,
            page_slots: slots,
            promotion_batch: 4,
            adjacency_degree: DEGREE,
            write_behind_batch: 4,
            write_backlog_cap: 8,
        },
    )
    .expect("tier opens")
}

fn fork_child(parent: &SimTier) -> SimTier {
    parent.fork(parent.store().fork()).expect("fork succeeds")
}

/// Requests + maintains until every id is selectable (bounded rounds —
/// re-requesting each round is the documented caller contract).
fn promote(tier: &mut SimTier, ids: &[u64]) {
    for _ in 0..10 {
        if ids.iter().all(|id| tier.is_selectable(PageId(*id))) {
            return;
        }
        for id in ids {
            tier.request(PageId(*id), 1);
        }
        tier.step_begin().expect("step");
        tier.maintain();
    }
    panic!("pages {ids:?} did not become selectable");
}

/// The device slot currently holding `id` in this handle's view.
fn slot_of(tier: &SimTier, id: u64) -> u32 {
    (0..tier.capacity())
        .find(|slot| tier.page_of_slot(*slot) == Some(PageId(id)))
        .expect("page resident")
}

#[test]
fn fork_shares_the_warm_working_set_without_copies() {
    let mut parent = tier_with(8, 6);
    promote(&mut parent, &[0, 1, 2, 3, 4, 5]);

    let copies_before = parent.backend().copies_enqueued();
    let mut child = fork_child(&parent);
    assert_eq!(
        parent.backend().copies_enqueued(),
        copies_before,
        "fork is metadata-only: no device copies"
    );

    for id in 0..6 {
        assert!(child.is_selectable(PageId(id)), "page {id} warm in child");
        assert_eq!(child.request(PageId(id), 1), RequestOutcome::Hit);
    }
    assert_eq!(child.stats().promotions_started, 0);
    assert_eq!(child.stats().hits, 6);
    assert_eq!(child.resident(), 6);
    assert_eq!(parent.resident(), 6);
}

#[test]
fn fork_refuses_an_unflushed_parent() {
    let mut parent = tier_with(4, 0);
    parent.append(&blob_for(0)).expect("append");

    let Err(error) = parent.fork(parent.store().fork()) else {
        panic!("unflushed parent must refuse");
    };
    assert_eq!(error.code(), "failed_precondition.tier.fork_unflushed");

    parent.flush().expect("flush");
    assert!(parent.fork(parent.store().fork()).is_ok(), "flushed: forks");
}

#[test]
fn fork_refuses_a_foreign_store() {
    let mut parent = tier_with(4, 2);
    promote(&mut parent, &[0, 1]);

    let Err(error) = parent.fork(InMemoryStore::new()) else {
        panic!("manifest-less store must refuse");
    };
    assert_eq!(error.code(), "invalid_argument.gpu.config");

    let mut mismatched = InMemoryStore::new();
    mismatched
        .write_manifest(TierManifest {
            page_bytes: PAGE_BYTES * 2,
            summary_bytes: SUMMARY_BYTES,
        })
        .expect("manifest");
    let Err(error) = parent.fork(mismatched) else {
        panic!("mismatched geometry must refuse");
    };
    assert_eq!(error.code(), "failed_precondition.tier.geometry_mismatch");
}

#[test]
fn shared_eviction_keeps_the_survivor_selectable() {
    let mut parent = tier_with(2, 4);
    promote(&mut parent, &[0, 1]);
    let child = fork_child(&parent);

    // Pressure the parent: the pool is full and both slots are shared, so
    // its evictions are reference releases — no validity flip, no reuse
    // gate, no headroom (the placement degrades until the child releases).
    parent.request(PageId(2), 1);
    parent.step_begin().expect("step");
    parent.maintain();

    assert!(parent.stats().evictions >= 1, "parent released references");
    assert_eq!(
        parent.stats().slots_reused,
        0,
        "no slot freed while the child holds them"
    );
    assert!(
        parent.stats().degraded_placements >= 1,
        "no headroom from shared releases: placement degrades"
    );
    assert_eq!(parent.gated(), 0, "shared releases stage no gates");
    for id in 0..2 {
        assert!(
            child.is_selectable(PageId(id)),
            "page {id} stays selectable for the child"
        );
    }
    assert_eq!(parent.free_now(), 0, "slots remain allocated to the union");
}

#[test]
fn last_release_frees_slots_for_the_family() {
    let mut parent = tier_with(2, 4);
    promote(&mut parent, &[0, 1]);
    let mut child = fork_child(&parent);

    // Keep page 1 hot in the child so page 0 is the coldest candidate.
    assert_eq!(child.request(PageId(1), 1), RequestOutcome::Hit);

    // Parent releases its references under pressure (shared: no frees).
    parent.request(PageId(2), 1);
    parent.step_begin().expect("step");
    parent.maintain();

    // Child pressure now holds the last references: its eviction flips
    // validity, unlinks, and gates on its epoch fence; the sweep frees the
    // slot and the promotion lands.
    promote(&mut child, &[2]);

    assert!(child.is_selectable(PageId(2)), "promotion landed");
    assert!(child.is_selectable(PageId(1)), "hot page survived");
    assert!(!child.is_selectable(PageId(0)), "cold page evicted");
    assert!(
        child.stats().slots_reused >= 1,
        "last release opened a reuse gate"
    );
}

#[test]
fn divergent_appends_get_unique_ids() {
    let mut parent = tier_with(8, 0);
    let mut child = fork_child(&parent);

    let parent_id = parent.append(&blob_for(10)).expect("parent append");
    let child_id = child.append(&blob_for(20)).expect("child append");
    assert_ne!(
        parent_id, child_id,
        "the id clock is shared: ids stay unique across the family"
    );

    parent.flush().expect("parent flush");
    child.flush().expect("child flush");
    assert!(
        parent.store().len() == 1 && child.store().len() == 1,
        "appends diverge: each branch of record gets its own page"
    );
}

#[test]
fn last_release_unlinks_union_adjacency() {
    let mut parent = tier_with(2, 4);
    // Page 1 carries an edge to page 0; promoting both links their slots.
    parent
        .store_mut()
        .seed(PageId(1), blob_with_edges(1, vec![PageId(0)]));
    promote(&mut parent, &[0]);
    promote(&mut parent, &[1]);
    let mut child = fork_child(&parent);

    let linked_row = slot_of(&child, 1);
    let row_offset = u64::from(linked_row) * u64::from(DEGREE) * 4;
    let row = child
        .backend_mut()
        .read_back(Region::Adjacency, row_offset, usize::from(DEGREE) * 4)
        .expect("read row");
    assert!(
        row.chunks(4).any(|entry| entry != [0xFF; 4]),
        "precondition: page 1's row links page 0's slot"
    );

    // Family-wide eviction of page 0: parent releases (shared), child's
    // release is the last — it must unlink page 0's slot from the union
    // adjacency, using links the parent's install created.
    assert_eq!(child.request(PageId(1), 1), RequestOutcome::Hit);
    parent.request(PageId(2), 1);
    parent.step_begin().expect("step");
    parent.maintain();
    promote(&mut child, &[2]);
    assert!(!child.is_selectable(PageId(0)), "page 0 left the union");

    let row = child
        .backend_mut()
        .read_back(Region::Adjacency, row_offset, usize::from(DEGREE) * 4)
        .expect("read row");
    assert!(
        row.chunks(4).all(|entry| entry == [0xFF; 4]),
        "page 1's device row no longer names the dead slot"
    );
}

#[test]
fn dropping_a_handle_releases_its_shared_references() {
    let mut parent = tier_with(2, 4);
    promote(&mut parent, &[0, 1]);
    let child = fork_child(&parent);
    drop(child);

    // With the child gone, the parent holds the last references again:
    // pressure evicts, gates, frees, and the new page lands.
    assert_eq!(parent.request(PageId(1), 1), RequestOutcome::Hit);
    promote(&mut parent, &[2]);
    assert!(parent.is_selectable(PageId(2)));
    assert!(
        parent.stats().slots_reused >= 1,
        "dropped handle's references no longer pin the slot"
    );
}

#[test]
fn union_accounting_is_conserved_across_handles() {
    let mut parent = tier_with(4, 8);
    promote(&mut parent, &[0, 1, 2, 3]);
    let mut child = fork_child(&parent);

    // Divergent churn on both handles.
    parent.request(PageId(4), 1);
    parent.step_begin().expect("step");
    parent.maintain();
    promote(&mut child, &[5]);
    parent.step_begin().expect("step");
    parent.maintain();

    // Global slot accounting: every allocated slot is either referenced by
    // some handle's table or held by some handle's reuse gate.
    let allocated = parent.capacity() - parent.free_now();
    let mut union_slots: Vec<u32> = (0..parent.capacity())
        .filter(|slot| parent.page_of_slot(*slot).is_some() || child.page_of_slot(*slot).is_some())
        .collect();
    union_slots.dedup();
    let gated = parent.gated() + child.gated();
    assert_eq!(
        allocated,
        u32::try_from(union_slots.len() + gated).unwrap(),
        "allocated = union-resident + family gates"
    );
}
