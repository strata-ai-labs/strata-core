use super::{PhysicalKey, RowError, StorageRow, StorageSpaceId};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

fn branch() -> BranchId {
    BranchId::from_bytes([1; BranchId::BYTE_LEN])
}

#[test]
fn storage_space_ids_enforce_v1_range_split() {
    assert_eq!(
        StorageSpaceId::from_raw(0),
        Err(RowError::InvalidStorageSpaceId { raw: 0 })
    );
    assert!(StorageSpaceId::COMMIT_TIMELINE.is_storage_owned());
    assert_eq!(
        StorageSpaceId::engine(0x01),
        Err(RowError::StorageReservedSpaceId { raw: 0x01 })
    );
    assert!(StorageSpaceId::engine(0x20)
        .expect("engine id")
        .is_engine_owned());

    for raw in 0x02..=0x1f {
        let id = StorageSpaceId::from_raw(raw).expect("reserved storage id");
        assert!(id.is_storage_owned(), "0x{raw:02x} should be storage-owned");
        assert_eq!(
            StorageSpaceId::engine(raw),
            Err(RowError::StorageReservedSpaceId { raw })
        );
    }
}

#[test]
fn physical_key_rejects_invalid_space() {
    let id = StorageSpaceId::engine(0x20).expect("engine id");

    assert_eq!(
        PhysicalKey::new(branch(), "", id, b"k".to_vec()).expect_err("empty"),
        RowError::EmptySpace
    );
    assert_eq!(
        PhysicalKey::new(branch(), "bad\0space", id, b"k".to_vec()).expect_err("nul"),
        RowError::SpaceContainsNul
    );
}

#[test]
fn storage_row_preserves_expiry_and_tombstone_facts() {
    let key = PhysicalKey::new(
        branch(),
        "default",
        StorageSpaceId::engine(0x20).expect("engine id"),
        b"k".to_vec(),
    )
    .expect("physical key");
    let row = StorageRow::put(
        key.clone(),
        CommitVersion::new(7),
        Timestamp::from_micros(11),
        Timestamp::from_micros(99),
        b"value".to_vec(),
    );
    let tombstone = StorageRow::tombstone(key, CommitVersion::new(8), Timestamp::from_micros(12));

    assert_eq!(row.expires_at(), Timestamp::from_micros(99));
    assert_eq!(row.value(), b"value");
    assert!(!row.is_tombstone());
    assert_eq!(tombstone.expires_at(), Timestamp::EPOCH);
    assert!(tombstone.value().is_empty());
    assert!(tombstone.is_tombstone());
}
