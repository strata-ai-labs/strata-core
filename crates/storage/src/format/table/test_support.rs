use super::super::{key::encode_internal_key, storage_row::encode_storage_row};
use crate::row::{InternalKey, PhysicalKey, StorageRow, StorageSpaceId};
use strata_core::{BranchId, CommitVersion, Timestamp};

pub(super) fn branch(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

pub(super) fn key(user_key: impl Into<Vec<u8>>) -> PhysicalKey {
    PhysicalKey::new(
        branch(7),
        "default",
        StorageSpaceId::engine(0x20).expect("engine storage space"),
        user_key,
    )
    .expect("physical key")
}

pub(super) fn alternate_key(user_key: impl Into<Vec<u8>>) -> PhysicalKey {
    PhysicalKey::new(
        branch(8),
        "default",
        StorageSpaceId::engine(0x20).expect("engine storage space"),
        user_key,
    )
    .expect("physical key")
}

pub(super) fn put(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    StorageRow::put(
        key(user_key),
        CommitVersion::new(version),
        timestamp(version),
        Timestamp::EPOCH,
        format!("value-{version}").into_bytes(),
    )
}

pub(super) fn tombstone(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    StorageRow::tombstone(
        key(user_key),
        CommitVersion::new(version),
        timestamp(version),
    )
}

pub(super) fn put_for_key(physical_key: PhysicalKey, version: u64) -> StorageRow {
    StorageRow::put(
        physical_key,
        CommitVersion::new(version),
        timestamp(version),
        Timestamp::EPOCH,
        format!("value-{version}").into_bytes(),
    )
}

pub(super) fn timestamp(version: u64) -> Timestamp {
    Timestamp::from_micros(1_700_000_000_000_000 + version)
}

pub(super) fn internal_key_for_row(row: &StorageRow) -> InternalKey {
    InternalKey::new(row.physical_key().clone(), row.commit_version())
}

pub(super) fn encoded_internal_key_for_row(row: &StorageRow) -> Vec<u8> {
    encode_internal_key(&internal_key_for_row(row))
}

pub(super) fn encoded_row(row: &StorageRow) -> Vec<u8> {
    encode_storage_row(row).expect("encode storage row")
}

pub(super) fn data_payload(parts: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&u32_len(parts.len()).to_le_bytes());
    for (key_bytes, row_bytes) in parts {
        bytes.extend_from_slice(&u32_len(key_bytes.len()).to_le_bytes());
        bytes.extend_from_slice(key_bytes);
        bytes.extend_from_slice(&u32_len(row_bytes.len()).to_le_bytes());
        bytes.extend_from_slice(row_bytes);
    }
    bytes
}

fn u32_len(len: usize) -> u32 {
    u32::try_from(len).expect("test length fits u32")
}
