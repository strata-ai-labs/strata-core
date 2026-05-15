use super::{
    key::{decode_internal_key, encode_internal_key},
    manifest::{decode_manifest, encode_manifest, DatabaseManifest},
    quarantine::{
        decode_quarantine_inventory, encode_quarantine_inventory, QuarantineEntry,
        QuarantineInventory,
    },
    segment_metadata::{decode_segment_metadata, encode_segment_metadata, SegmentMetadata},
    snapshot::{
        decode_snapshot_container, decode_snapshot_header, decode_snapshot_section,
        encode_snapshot_container, encode_snapshot_header, encode_snapshot_section,
        SnapshotContainer, SnapshotHeader, SnapshotSection,
    },
    storage_row::{decode_storage_row, encode_storage_row},
    wal::{
        decode_wal_record, decode_wal_record_envelope, decode_wal_segment_header,
        encode_wal_record, encode_wal_record_envelope, encode_wal_segment_header, WalRecord,
        WalRecordEnvelope, WalSegmentHeader,
    },
    watermark::{decode_snapshot_watermark, encode_snapshot_watermark, SnapshotWatermark},
};
use crate::object::ObjectName;
use crate::row::{InternalKey, PhysicalKey, StorageRow, StorageSpaceId};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const INTERNAL_KEY_ORDINARY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/internal-key-ordinary.hex");
const INTERNAL_KEY_ZERO_USER_BYTE: &str =
    include_str!("../../testdata/goldens/storage-format-v1/internal-key-zero-user-byte.hex");
const STORAGE_ROW_PUT: &str =
    include_str!("../../testdata/goldens/storage-format-v1/storage-row-put.hex");
const STORAGE_ROW_TOMBSTONE: &str =
    include_str!("../../testdata/goldens/storage-format-v1/storage-row-tombstone.hex");
const DATABASE_IDENTITY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/manifest-identity.hex");
const QUARANTINE_INVENTORY_EMPTY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/quarantine-inventory-empty.hex");
const QUARANTINE_INVENTORY_MULTI_ENTRY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/quarantine-inventory-multi-entry.hex");
const SNAPSHOT_WATERMARK_EMPTY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/snapshot-watermark-empty.hex");
const SNAPSHOT_WATERMARK_PRESENT: &str =
    include_str!("../../testdata/goldens/storage-format-v1/snapshot-watermark-present.hex");
const SNAPSHOT_HEADER_IDENTITY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/snapshot-header-identity.hex");
const SNAPSHOT_SECTION_EMPTY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/snapshot-section-empty.hex");
const SNAPSHOT_CONTAINER_SINGLE_SECTION: &str =
    include_str!("../../testdata/goldens/storage-format-v1/snapshot-container-single-section.hex");
const SEGMENT_METADATA_SIDECAR: &str =
    include_str!("../../testdata/goldens/storage-format-v1/segment-metadata-sidecar.hex");
const WAL_SEGMENT_HEADER: &str =
    include_str!("../../testdata/goldens/storage-format-v1/wal-segment-header.hex");
const WAL_RECORD_EMPTY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/wal-record-empty.hex");
const WAL_RECORD_PAYLOAD: &str =
    include_str!("../../testdata/goldens/storage-format-v1/wal-record-payload.hex");
const WAL_RECORD_ENVELOPE: &str =
    include_str!("../../testdata/goldens/storage-format-v1/wal-record-envelope.hex");

#[test]
fn internal_key_ordinary_matches_golden_vector() {
    let key = InternalKey::new(ordinary_key(), CommitVersion::new(42));
    let golden = parse_hex(INTERNAL_KEY_ORDINARY);

    assert_eq!(encode_internal_key(&key), golden);
    assert_eq!(decode_internal_key(&golden), Ok(key));
}

#[test]
fn internal_key_zero_user_byte_matches_golden_vector() {
    let key = InternalKey::new(zero_user_byte_key(), CommitVersion::new(7));
    let golden = parse_hex(INTERNAL_KEY_ZERO_USER_BYTE);

    assert_eq!(encode_internal_key(&key), golden);
    assert_eq!(decode_internal_key(&golden), Ok(key));
}

#[test]
fn storage_row_put_matches_golden_vector() {
    let row = StorageRow::put(
        ordinary_key(),
        CommitVersion::new(42),
        Timestamp::from_micros(1_700_000_000_123_456),
        Timestamp::EPOCH,
        b"value".to_vec(),
    );
    let golden = parse_hex(STORAGE_ROW_PUT);

    assert_eq!(encode_storage_row(&row).expect("encode row"), golden);
    assert_eq!(decode_storage_row(&golden), Ok(row));
}

#[test]
fn storage_row_tombstone_matches_golden_vector() {
    let row = StorageRow::tombstone(
        ordinary_key(),
        CommitVersion::new(43),
        Timestamp::from_micros(1_700_000_000_123_457),
    );
    let golden = parse_hex(STORAGE_ROW_TOMBSTONE);

    assert_eq!(encode_storage_row(&row).expect("encode row"), golden);
    assert_eq!(decode_storage_row(&golden), Ok(row));
}

#[test]
fn manifest_identity_matches_golden_vector() {
    let manifest = DatabaseManifest::new(
        [
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ],
        "identity",
    )
    .expect("database format")
    .with_recovery_facts(5, Some(42), Some(3), Some(CommitVersion::new(41)))
    .expect("recovery facts");
    let golden = parse_hex(DATABASE_IDENTITY);

    assert_eq!(encode_manifest(&manifest).expect("encode manifest"), golden);
    assert_eq!(decode_manifest(&golden), Ok(manifest));
}

#[test]
fn quarantine_inventory_empty_matches_golden_vector() {
    let inventory =
        QuarantineInventory::new(database_id(), ordinary_branch_id(), "identity", vec![])
            .expect("quarantine inventory");
    let golden = parse_hex(QUARANTINE_INVENTORY_EMPTY);

    assert_eq!(
        encode_quarantine_inventory(&inventory).expect("encode quarantine inventory"),
        golden
    );
    assert_eq!(decode_quarantine_inventory(&golden), Ok(inventory));
}

#[test]
fn quarantine_inventory_multi_entry_matches_golden_vector() {
    let inventory = QuarantineInventory::new(
        database_id(),
        ordinary_branch_id(),
        "identity",
        vec![
            quarantine_entry(
                "table0002",
                "tables/main/l0001/table0002",
                256,
                Timestamp::from_micros(1_700_000_000_000_100),
            ),
            quarantine_entry(
                "table0001",
                "tables/main/l0000/table0001",
                128,
                Timestamp::from_micros(1_700_000_000_000_000),
            ),
        ],
    )
    .expect("quarantine inventory");
    let golden = parse_hex(QUARANTINE_INVENTORY_MULTI_ENTRY);

    assert_eq!(
        encode_quarantine_inventory(&inventory).expect("encode quarantine inventory"),
        golden
    );
    assert_eq!(decode_quarantine_inventory(&golden), Ok(inventory));
}

#[test]
fn snapshot_watermark_empty_matches_golden_vector() {
    let golden = parse_hex(SNAPSHOT_WATERMARK_EMPTY);

    assert_eq!(
        encode_snapshot_watermark(SnapshotWatermark::Empty).expect("encode watermark"),
        golden
    );
    assert_eq!(
        decode_snapshot_watermark(&golden),
        Ok(SnapshotWatermark::Empty)
    );
}

#[test]
fn snapshot_watermark_present_matches_golden_vector() {
    let watermark = SnapshotWatermark::present(
        3,
        CommitVersion::new(42),
        Timestamp::from_micros(1_700_000_000_123_456),
    )
    .expect("watermark");
    let golden = parse_hex(SNAPSHOT_WATERMARK_PRESENT);

    assert_eq!(
        encode_snapshot_watermark(watermark).expect("encode watermark"),
        golden
    );
    assert_eq!(decode_snapshot_watermark(&golden), Ok(watermark));
}

#[test]
fn segment_metadata_sidecar_matches_golden_vector() {
    let mut metadata = SegmentMetadata::empty(5);
    metadata.track_record(
        CommitVersion::new(7),
        Timestamp::from_micros(1_700_000_000_000_000),
    );
    metadata.track_record(
        CommitVersion::new(11),
        Timestamp::from_micros(1_700_000_000_123_456),
    );
    let golden = parse_hex(SEGMENT_METADATA_SIDECAR);

    assert_eq!(encode_segment_metadata(&metadata), golden);
    assert_eq!(decode_segment_metadata(&golden), Ok(metadata));
}

#[test]
fn wal_segment_header_matches_golden_vector() {
    let header = WalSegmentHeader::new(
        5,
        [
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ],
    );
    let golden = parse_hex(WAL_SEGMENT_HEADER);

    assert_eq!(encode_wal_segment_header(&header), golden);
    assert_eq!(
        decode_wal_segment_header(&golden, Some(5)),
        Ok((header, golden.len()))
    );
}

#[test]
fn wal_record_empty_matches_golden_vector() {
    let record = WalRecord::new(
        CommitVersion::new(41),
        BranchId::from_bytes([
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ]),
        Timestamp::from_micros(1_700_000_000_123_456),
        Vec::new(),
    );
    let golden = parse_hex(WAL_RECORD_EMPTY);

    assert_eq!(
        encode_wal_record(&record).expect("encode WAL record"),
        golden
    );
    assert_eq!(decode_wal_record(&golden), Ok((record, golden.len())));
}

#[test]
fn wal_record_payload_matches_golden_vector() {
    let record = payload_wal_record();
    let golden = parse_hex(WAL_RECORD_PAYLOAD);

    assert_eq!(
        encode_wal_record(&record).expect("encode WAL record"),
        golden
    );
    assert_eq!(decode_wal_record(&golden), Ok((record, golden.len())));
}

#[test]
fn wal_record_envelope_matches_golden_vector() {
    let record_bytes = encode_wal_record(&payload_wal_record()).expect("encode WAL record");
    let envelope = WalRecordEnvelope::new(record_bytes).expect("envelope");
    let golden = parse_hex(WAL_RECORD_ENVELOPE);

    assert_eq!(
        encode_wal_record_envelope(&envelope).expect("encode WAL envelope"),
        golden
    );
    assert_eq!(
        decode_wal_record_envelope(&golden),
        Ok((envelope, golden.len()))
    );
}

#[test]
fn snapshot_header_identity_matches_golden_vector() {
    let header = snapshot_header();
    let golden = parse_hex(SNAPSHOT_HEADER_IDENTITY);

    assert_eq!(
        encode_snapshot_header(&header).expect("encode snapshot header"),
        golden
    );
    assert_eq!(decode_snapshot_header(&golden), Ok((header, golden.len())));
}

#[test]
fn snapshot_section_empty_matches_golden_vector() {
    let section = SnapshotSection::new(0x01, Vec::new()).expect("section");
    let golden = parse_hex(SNAPSHOT_SECTION_EMPTY);

    assert_eq!(
        encode_snapshot_section(&section).expect("encode snapshot section"),
        golden
    );
    assert_eq!(
        decode_snapshot_section(&golden),
        Ok((section, golden.len()))
    );
}

#[test]
fn snapshot_container_single_section_matches_golden_vector() {
    let container = SnapshotContainer::new(
        snapshot_header(),
        vec![SnapshotSection::new(0x01, b"rows".to_vec()).expect("section")],
    );
    let golden = parse_hex(SNAPSHOT_CONTAINER_SINGLE_SECTION);

    assert_eq!(
        encode_snapshot_container(&container).expect("encode snapshot container"),
        golden
    );
    assert_eq!(decode_snapshot_container(&golden), Ok(container));
}

fn ordinary_key() -> PhysicalKey {
    PhysicalKey::new(
        ordinary_branch_id(),
        "default",
        StorageSpaceId::engine(0x20).expect("engine id"),
        b"alpha".to_vec(),
    )
    .expect("ordinary key")
}

fn zero_user_byte_key() -> PhysicalKey {
    PhysicalKey::new(
        BranchId::from_bytes([
            0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
            0x1e, 0x1f,
        ]),
        "timeline",
        StorageSpaceId::COMMIT_TIMELINE,
        [0x00, 0x41, 0x00],
    )
    .expect("zero-byte key")
}

fn snapshot_header() -> SnapshotHeader {
    SnapshotHeader::new(
        3,
        CommitVersion::new(42),
        Timestamp::from_micros(1_700_000_000_123_456),
        [
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ],
        "identity",
    )
    .expect("snapshot header")
}

fn payload_wal_record() -> WalRecord {
    WalRecord::new(
        CommitVersion::new(42),
        BranchId::from_bytes([
            0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
            0x1e, 0x1f,
        ]),
        Timestamp::from_micros(1_700_000_000_123_457),
        b"payload".to_vec(),
    )
}

fn database_id() -> [u8; 16] {
    [
        0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
        0x2f,
    ]
}

fn ordinary_branch_id() -> BranchId {
    BranchId::from_bytes([
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f,
    ])
}

fn quarantine_entry(
    object_id: &str,
    source_object: &str,
    byte_count: u64,
    quarantined_at: Timestamp,
) -> QuarantineEntry {
    QuarantineEntry::new(
        object_id,
        ObjectName::new(source_object).expect("source object"),
        byte_count,
        quarantined_at,
    )
    .expect("quarantine entry")
}

fn parse_hex(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for line in text.lines() {
        let data = line.split_once('#').map_or(line, |(data, _comment)| data);
        for token in data.split_whitespace() {
            bytes.push(u8::from_str_radix(token, 16).expect("valid hex byte"));
        }
    }
    bytes
}
