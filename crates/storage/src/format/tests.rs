use super::{
    branch_catalog_manifest::{
        BranchCatalogEntry, BranchCatalogManifest, BranchCatalogParent, BranchCatalogStatus,
    },
    key::{decode_internal_key, encode_internal_key},
    manifest::{decode_manifest, encode_manifest, DatabaseManifest},
    pending_releases_manifest::{
        decode_pending_releases_manifest, encode_pending_releases_manifest, PendingReleasesEntry,
        PendingReleasesManifest,
    },
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
    table_manifest::{
        decode_table_manifest, encode_table_manifest, TableManifest, TableManifestExtensionSection,
        TableManifestInheritedLayer, TableManifestInheritedLayerStatus, TableManifestLevel,
        TableManifestTableBounds, TableManifestTableFacts, TableManifestTableProvenance,
        TableManifestTableRef,
    },
    wal::{
        decode_wal_commit_payload, decode_wal_record, decode_wal_record_envelope,
        decode_wal_segment_header, encode_wal_commit_payload, encode_wal_record,
        encode_wal_record_envelope, encode_wal_segment_header, WalCommitPayload, WalRecord,
        WalRecordEnvelope, WalSegmentHeader,
    },
    watermark::{decode_snapshot_watermark, encode_snapshot_watermark, SnapshotWatermark},
};
use crate::object::ObjectName;
use crate::row::{InternalKey, PhysicalKey, StorageRow, StorageSpaceId};
use std::fs;
use strata_core::{BranchId, CommitVersion, Timestamp};

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
const BRANCH_CATALOG_MANIFEST_EMPTY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/branch-catalog-manifest-empty.hex");
const BRANCH_CATALOG_MANIFEST_SINGLE_ACTIVE: &str = include_str!(
    "../../testdata/goldens/storage-format-v1/branch-catalog-manifest-single-active.hex"
);
const BRANCH_CATALOG_MANIFEST_ACTIVE_AND_DELETED: &str = include_str!(
    "../../testdata/goldens/storage-format-v1/branch-catalog-manifest-active-and-deleted.hex"
);
const BRANCH_CATALOG_MANIFEST_WITH_PARENT: &str = include_str!(
    "../../testdata/goldens/storage-format-v1/branch-catalog-manifest-with-parent.hex"
);
const PENDING_RELEASES_MANIFEST_EMPTY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/pending-releases-manifest-empty.hex");
const PENDING_RELEASES_MANIFEST_SINGLE: &str =
    include_str!("../../testdata/goldens/storage-format-v1/pending-releases-manifest-single.hex");
const PENDING_RELEASES_MANIFEST_MULTI: &str =
    include_str!("../../testdata/goldens/storage-format-v1/pending-releases-manifest-multi.hex");
const QUARANTINE_INVENTORY_EMPTY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/quarantine-inventory-empty.hex");
const QUARANTINE_INVENTORY_MULTI_ENTRY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/quarantine-inventory-multi-entry.hex");
const TABLE_MANIFEST_EMPTY: &str =
    include_str!("../../testdata/goldens/storage-format-v1/table-manifest-empty.hex");
const TABLE_MANIFEST_OWNED_LEVELS: &str =
    include_str!("../../testdata/goldens/storage-format-v1/table-manifest-owned-levels.hex");
const TABLE_MANIFEST_INHERITED_LAYERS: &str =
    include_str!("../../testdata/goldens/storage-format-v1/table-manifest-inherited-layers.hex");
const TABLE_MANIFEST_MATERIALIZATION_PROVENANCE: &str = include_str!(
    "../../testdata/goldens/storage-format-v1/table-manifest-materialization-provenance.hex"
);
const TABLE_MANIFEST_EXTENSION_SECTION: &str =
    include_str!("../../testdata/goldens/storage-format-v1/table-manifest-extension-section.hex");
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
const WAL_COMMIT_PAYLOAD_ONE_PUT: &str =
    include_str!("../../testdata/goldens/storage-format-v1/wal-commit-payload-one-put.hex");
const WAL_COMMIT_PAYLOAD_PUT_TOMBSTONE: &str =
    include_str!("../../testdata/goldens/storage-format-v1/wal-commit-payload-put-tombstone.hex");
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
fn branch_catalog_manifest_empty_matches_golden_vector() {
    let manifest =
        BranchCatalogManifest::new(branch_catalog_database_id(), 1, Vec::new()).expect("manifest");
    let golden = parse_hex(BRANCH_CATALOG_MANIFEST_EMPTY);

    assert_eq!(
        super::encode_branch_catalog_manifest(&manifest).expect("encode branch catalog manifest"),
        golden
    );
    assert_eq!(super::decode_branch_catalog_manifest(&golden), Ok(manifest));
}

#[test]
fn branch_catalog_manifest_single_active_matches_golden_vector() {
    let active = BranchCatalogEntry::new(repeated_branch_id(0x10), 3, BranchCatalogStatus::Active)
        .expect("active")
        .with_created_at(7)
        .expect("created")
        .with_state_revision(2);
    let manifest = BranchCatalogManifest::new(branch_catalog_database_id(), 5, vec![active])
        .expect("manifest");
    let golden = parse_hex(BRANCH_CATALOG_MANIFEST_SINGLE_ACTIVE);

    assert_eq!(
        super::encode_branch_catalog_manifest(&manifest).expect("encode branch catalog manifest"),
        golden
    );
    assert_eq!(super::decode_branch_catalog_manifest(&golden), Ok(manifest));
}

#[test]
fn branch_catalog_manifest_active_and_deleted_matches_golden_vector() {
    let active = BranchCatalogEntry::new(repeated_branch_id(0x10), 1, BranchCatalogStatus::Active)
        .expect("active");
    let deleted =
        BranchCatalogEntry::new(repeated_branch_id(0x22), 4, BranchCatalogStatus::Deleted)
            .expect("deleted")
            .with_deleted_at(11)
            .expect("deleted timestamp");
    let manifest =
        BranchCatalogManifest::new(branch_catalog_database_id(), 7, vec![active, deleted])
            .expect("manifest");
    let golden = parse_hex(BRANCH_CATALOG_MANIFEST_ACTIVE_AND_DELETED);

    assert_eq!(
        super::encode_branch_catalog_manifest(&manifest).expect("encode branch catalog manifest"),
        golden
    );
    assert_eq!(super::decode_branch_catalog_manifest(&golden), Ok(manifest));
}

#[test]
fn branch_catalog_manifest_with_parent_matches_golden_vector() {
    let entry = BranchCatalogEntry::new(repeated_branch_id(0x40), 2, BranchCatalogStatus::Active)
        .expect("active")
        .with_parent(BranchCatalogParent::new(repeated_branch_id(0x10), 9))
        .with_created_at(10)
        .expect("created timestamp");
    let manifest =
        BranchCatalogManifest::new(branch_catalog_database_id(), 3, vec![entry]).expect("manifest");
    let golden = parse_hex(BRANCH_CATALOG_MANIFEST_WITH_PARENT);

    assert_eq!(
        super::encode_branch_catalog_manifest(&manifest).expect("encode branch catalog manifest"),
        golden
    );
    assert_eq!(super::decode_branch_catalog_manifest(&golden), Ok(manifest));
}

#[test]
fn pending_releases_manifest_empty_matches_golden_vector() {
    let manifest = PendingReleasesManifest::new(pending_releases_database_id(), 1, Vec::new())
        .expect("manifest");
    let golden = parse_hex(PENDING_RELEASES_MANIFEST_EMPTY);

    assert_eq!(
        encode_pending_releases_manifest(&manifest).expect("encode pending releases manifest"),
        golden
    );
    assert_eq!(decode_pending_releases_manifest(&golden), Ok(manifest));
}

#[test]
fn pending_releases_manifest_single_matches_golden_vector() {
    let entry = PendingReleasesEntry::new(repeated_branch_id(0x21), vec!["table-alpha".to_owned()])
        .expect("entry");
    let manifest = PendingReleasesManifest::new(pending_releases_database_id(), 5, vec![entry])
        .expect("manifest");
    let golden = parse_hex(PENDING_RELEASES_MANIFEST_SINGLE);

    assert_eq!(
        encode_pending_releases_manifest(&manifest).expect("encode pending releases manifest"),
        golden
    );
    assert_eq!(decode_pending_releases_manifest(&golden), Ok(manifest));
}

#[test]
fn pending_releases_manifest_multi_matches_golden_vector() {
    let first = PendingReleasesEntry::new(
        repeated_branch_id(0x11),
        vec!["table-a-1".to_owned(), "table-a-2".to_owned()],
    )
    .expect("first entry");
    let second = PendingReleasesEntry::new(repeated_branch_id(0x22), vec!["table-b-1".to_owned()])
        .expect("second entry");
    let manifest =
        PendingReleasesManifest::new(pending_releases_database_id(), 9, vec![first, second])
            .expect("manifest");
    let golden = parse_hex(PENDING_RELEASES_MANIFEST_MULTI);

    assert_eq!(
        encode_pending_releases_manifest(&manifest).expect("encode pending releases manifest"),
        golden
    );
    assert_eq!(decode_pending_releases_manifest(&golden), Ok(manifest));
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
fn table_manifest_empty_matches_golden_vector() {
    let manifest = TableManifest::new(
        BranchId::from_bytes([0x11; BranchId::BYTE_LEN]),
        None,
        1,
        vec![],
        vec![],
        vec![],
    )
    .expect("table manifest");
    let golden = parse_hex(TABLE_MANIFEST_EMPTY);

    assert_eq!(
        encode_table_manifest(&manifest).expect("encode table manifest"),
        golden
    );
    assert_eq!(decode_table_manifest(&golden), Ok(manifest));
}

#[test]
fn table_manifest_owned_levels_matches_golden_vector() {
    let manifest = table_manifest_owned_levels();
    let golden = parse_hex(TABLE_MANIFEST_OWNED_LEVELS);

    assert_eq!(
        encode_table_manifest(&manifest).expect("encode table manifest"),
        golden
    );
    assert_eq!(decode_table_manifest(&golden), Ok(manifest));
}

#[test]
fn table_manifest_inherited_layers_matches_golden_vector() {
    let manifest = table_manifest_inherited_layers();
    let golden = parse_hex(TABLE_MANIFEST_INHERITED_LAYERS);

    assert_eq!(
        encode_table_manifest(&manifest).expect("encode table manifest"),
        golden
    );
    assert_eq!(decode_table_manifest(&golden), Ok(manifest));
}

#[test]
fn table_manifest_materialization_provenance_matches_golden_vector() {
    let manifest = table_manifest_materialization_provenance();
    let golden = parse_hex(TABLE_MANIFEST_MATERIALIZATION_PROVENANCE);

    assert_eq!(
        encode_table_manifest(&manifest).expect("encode table manifest"),
        golden
    );
    assert_eq!(decode_table_manifest(&golden), Ok(manifest));
}

#[test]
fn table_manifest_extension_section_matches_golden_vector() {
    let manifest = table_manifest_extension_section();
    let golden = parse_hex(TABLE_MANIFEST_EXTENSION_SECTION);

    assert_eq!(
        encode_table_manifest(&manifest).expect("encode table manifest"),
        golden
    );
    assert_eq!(decode_table_manifest(&golden), Ok(manifest));
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
fn wal_commit_payload_one_put_matches_golden_vector() {
    let payload = wal_commit_payload_one_put();
    let golden = parse_hex(WAL_COMMIT_PAYLOAD_ONE_PUT);

    assert_eq!(
        encode_wal_commit_payload(&payload).expect("encode WAL commit payload"),
        golden
    );
    assert_eq!(decode_wal_commit_payload(&golden), Ok(payload));
}

#[test]
fn wal_commit_payload_put_tombstone_matches_golden_vector() {
    let payload = wal_commit_payload_put_tombstone();
    let golden = parse_hex(WAL_COMMIT_PAYLOAD_PUT_TOMBSTONE);

    assert_eq!(
        encode_wal_commit_payload(&payload).expect("encode WAL commit payload"),
        golden
    );
    assert_eq!(decode_wal_commit_payload(&golden), Ok(payload));
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
fn wal_record_rejects_historical_empty_payload_golden() {
    let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/goldens/storage-format-v1")
        .join(format!("wal-record-empty-pre-m{}f.hex", 3));
    let historical_empty_payload =
        fs::read_to_string(&golden_path).expect("read historical empty payload golden");
    let bytes = parse_hex(&historical_empty_payload);

    assert!(matches!(
        decode_wal_record(&bytes),
        Err(super::FormatError::InvalidLength {
            field: "wal_record_len",
        })
    ));
}

fn wal_commit_payload_one_put() -> WalCommitPayload {
    let commit_version = CommitVersion::new(42);
    let commit_timestamp = Timestamp::from_micros(1_700_000_000_123_456);
    WalCommitPayload::new(vec![StorageRow::put(
        ordinary_key(),
        commit_version,
        commit_timestamp,
        Timestamp::EPOCH,
        b"value".to_vec(),
    )])
    .expect("WAL commit payload")
}

fn wal_commit_payload_put_tombstone() -> WalCommitPayload {
    let commit_version = CommitVersion::new(42);
    let commit_timestamp = Timestamp::from_micros(1_700_000_000_123_456);
    WalCommitPayload::new(vec![
        StorageRow::put(
            ordinary_key(),
            commit_version,
            commit_timestamp,
            Timestamp::EPOCH,
            b"value".to_vec(),
        ),
        StorageRow::tombstone(
            PhysicalKey::new(
                ordinary_branch_id(),
                "default",
                StorageSpaceId::engine(0x20).expect("engine id"),
                b"beta".to_vec(),
            )
            .expect("beta key"),
            commit_version,
            commit_timestamp,
        ),
    ])
    .expect("WAL commit payload")
}

fn payload_wal_record() -> WalRecord {
    WalRecord::new(
        CommitVersion::new(42),
        ordinary_branch_id(),
        Timestamp::from_micros(1_700_000_000_123_456),
        wal_commit_payload_one_put(),
    )
    .expect("WAL record")
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

fn table_manifest_owned_levels() -> TableManifest {
    let branch = BranchId::from_bytes([0x11; BranchId::BYTE_LEN]);
    TableManifest::new(
        branch,
        Some(2),
        5,
        vec![
            TableManifestLevel::new(
                crate::branch::facts::BranchLevel::ZERO,
                vec![
                    table_manifest_ref(
                        branch,
                        0,
                        "newer",
                        0,
                        b"k2",
                        b"k3",
                        TableManifestTableProvenance::Flush,
                    ),
                    table_manifest_ref(
                        branch,
                        0,
                        "older",
                        1,
                        b"k0",
                        b"k1",
                        TableManifestTableProvenance::Flush,
                    ),
                ],
            )
            .expect("l0"),
            TableManifestLevel::new(
                crate::branch::facts::BranchLevel::new(1),
                vec![
                    table_manifest_ref(
                        branch,
                        1,
                        "l1a",
                        0,
                        b"a",
                        b"b",
                        TableManifestTableProvenance::Flush,
                    ),
                    table_manifest_ref(
                        branch,
                        1,
                        "l1b",
                        1,
                        b"m",
                        b"n",
                        TableManifestTableProvenance::Flush,
                    ),
                ],
            )
            .expect("l1"),
        ],
        vec![],
        vec![],
    )
    .expect("table manifest")
}

fn table_manifest_inherited_layers() -> TableManifest {
    let branch = BranchId::from_bytes([0x11; BranchId::BYTE_LEN]);
    let first_source = BranchId::from_bytes([0x22; BranchId::BYTE_LEN]);
    let second_source = BranchId::from_bytes([0x33; BranchId::BYTE_LEN]);
    let first = TableManifestInheritedLayer::new(
        0,
        first_source,
        Some(1),
        CommitVersion::new(10),
        TableManifestInheritedLayerStatus::Active,
        vec![
            TableManifestLevel::new(
                crate::branch::facts::BranchLevel::ZERO,
                vec![table_manifest_ref(
                    first_source,
                    0,
                    "ancestor-l0",
                    0,
                    b"a",
                    b"b",
                    TableManifestTableProvenance::Flush,
                )],
            )
            .expect("ancestor l0"),
            TableManifestLevel::new(
                crate::branch::facts::BranchLevel::new(1),
                vec![table_manifest_ref(
                    first_source,
                    1,
                    "ancestor-l1",
                    0,
                    b"m",
                    b"n",
                    TableManifestTableProvenance::Flush,
                )],
            )
            .expect("ancestor l1"),
        ],
    )
    .expect("first inherited layer");
    let second = TableManifestInheritedLayer::new(
        1,
        second_source,
        Some(2),
        CommitVersion::new(20),
        TableManifestInheritedLayerStatus::Materializing,
        vec![TableManifestLevel::new(
            crate::branch::facts::BranchLevel::ZERO,
            vec![table_manifest_ref(
                second_source,
                0,
                "layer-1",
                0,
                b"a",
                b"b",
                TableManifestTableProvenance::Flush,
            )],
        )
        .expect("second l0")],
    )
    .expect("second inherited layer");
    TableManifest::new(branch, Some(3), 7, vec![], vec![first, second], vec![])
        .expect("table manifest")
}

fn table_manifest_materialization_provenance() -> TableManifest {
    let branch = BranchId::from_bytes([0x11; BranchId::BYTE_LEN]);
    let source = BranchId::from_bytes([0x44; BranchId::BYTE_LEN]);
    let provenance =
        TableManifestTableProvenance::materialization_replacement(source, CommitVersion::new(55))
            .expect("provenance");
    TableManifest::new(
        branch,
        None,
        1,
        vec![TableManifestLevel::new(
            crate::branch::facts::BranchLevel::ZERO,
            vec![table_manifest_ref(
                branch,
                0,
                "replacement",
                0,
                b"a",
                b"b",
                provenance,
            )],
        )
        .expect("l0")],
        vec![],
        vec![],
    )
    .expect("table manifest")
}

fn table_manifest_extension_section() -> TableManifest {
    TableManifest::new(
        BranchId::from_bytes([0x11; BranchId::BYTE_LEN]),
        None,
        1,
        vec![],
        vec![],
        vec![
            TableManifestExtensionSection::optional("audit.fact", true, b"abc".to_vec())
                .expect("extension"),
        ],
    )
    .expect("table manifest")
}

fn table_manifest_ref(
    branch: BranchId,
    level: u8,
    identity: &str,
    order: u32,
    physical_first: &[u8],
    physical_last: &[u8],
    provenance: TableManifestTableProvenance,
) -> TableManifestTableRef {
    let object = format!("tables/{branch}/l{level:04}/{identity}");
    TableManifestTableRef::new(
        crate::table::TableIdentity::new(identity).expect("identity"),
        ObjectName::new(object).expect("object"),
        order,
        TableManifestTableFacts::new(
            128,
            4,
            1,
            CommitVersion::new(1),
            CommitVersion::new(4),
            Some(Timestamp::from_micros(10)),
            Some(Timestamp::from_micros(40)),
        )
        .expect("facts"),
        TableManifestTableBounds::new(
            physical_first.to_vec(),
            physical_last.to_vec(),
            [physical_first, b":i0"].concat(),
            [physical_last, b":i9"].concat(),
        )
        .expect("bounds"),
        provenance,
    )
    .expect("table ref")
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

fn repeated_branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn branch_catalog_database_id() -> [u8; 16] {
    [0xAB; 16]
}

fn pending_releases_database_id() -> [u8; 16] {
    [0xCD; 16]
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
    let hex: String = text
        .lines()
        .map(|line| line.split_once('#').map_or(line, |(data, _comment)| data))
        .flat_map(str::chars)
        .filter(|ch| !ch.is_whitespace())
        .collect();

    assert_eq!(hex.len() % 2, 0, "hex fixture has odd byte count");
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("valid hex byte"))
        .collect()
}
